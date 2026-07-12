"""Визуальный проход: снять окно Terminal и получить вердикт зрения (спека §7, §8).
Ядро (`run_visual`) тестируется здесь через инъекцию `run_cmd`/`read_pixels` без GUI.
GUI-оркестрация (`observe_visual_all`) — e2e-only (реальный Terminal.app), обёрнута в
fail-safe: любая беда → блокирующий вердикт, никогда тихий pass (§8)."""
from __future__ import annotations

from pathlib import Path

from .capture import is_blank_frame, screencapture_cmd
from .visual_verdict import ChecklistItem, VisionVerdict


def blocking_verdict(reason: str) -> VisionVerdict:
    """Вердикт-блокиратор: один проваленный required-пункт → verdict_passes=False."""
    return VisionVerdict(
        issues=[],
        checklist=[ChecklistItem(check=reason, required=True, passed=False, note=reason)],
        open_critique=reason,
        raw=reason,
    )


def run_visual(cgwindow_id, vision, run_cmd, read_pixels, out_path: Path, prompt=None) -> VisionVerdict:
    """Снять окно `cgwindow_id` и оценить зрением. `run_cmd(list)->int`, `read_pixels(path)->bytes`
    инъектируются (в проде — subprocess/декод PNG; в тестах — фейки). Любая беда → блок (§8)."""
    from .vision import DEFAULT_VISION_PROMPT

    code = run_cmd(screencapture_cmd(cgwindow_id, out_path))
    if code != 0:
        return blocking_verdict(f"screencapture код {code} (нет Screen Recording?)")
    if is_blank_frame(read_pixels(out_path)):
        return blocking_verdict("кадр пустой/чёрный — вероятно нет разрешения на запись экрана")
    try:
        return vision.analyze_image(out_path, prompt or DEFAULT_VISION_PROMPT)
    except Exception as e:  # непарсящийся вердикт / недоступность бэкенда → блок
        return blocking_verdict(f"vision-бэкенд: {e}")


# --- ниже e2e-only: реальный Terminal.app. Не вызывается в headless-тестах/мок-смоуке ---
# (run_loop гардирует вызов через cfg.vision). Каждый сценарий обёрнут в fail-safe.


def observe_visual_all(cfg, fresh):
    """Для каждого сценария поднять fresh в Terminal.app, снять окно, оценить зрением.
    Любая беда сценария → блокирующий вердикт (§8), петля не падает."""
    verdicts = []
    for scenario in cfg.scenarios:
        try:
            verdicts.append(_observe_one(cfg, fresh, scenario))
        except Exception as e:
            verdicts.append(blocking_verdict(f"визуальный проход упал: {e}"))
    return verdicts


def gui_capture_verdict(binary, cwd, profile, vision, steps=(), settle_s=0.4, prompt=None):
    """Единственный GUI-проход: поднять бинарь в окне Terminal.app, снять окно, оценить зрением.

    Безопасность (то, ради чего это переписано):
    * НИКАКИХ System Events — общение с окном только через `do script … in window id`,
      то есть в tty конкретного окна. Глобальная инъекция клавиш в фоновом прогоне могла
      прилететь в чужое приложение. Заодно Accessibility больше не нужен.
    * Изолированный CLAVE_HOME — иначе наблюдаемый бинарь лез бы в реальные конфиг и чаты
      пользователя. Плюс детерминизм: свежий home → всегда одинаковый стартовый экран.
    * Без `activate` — фокус у пользователя не воруем.
    """
    import subprocess
    import tempfile
    import time
    import uuid

    from .terminal_driver import (
        close_window_applescript,
        launch_applescript,
        send_line_applescript,
    )
    from .terminal_profile import apply_bounds_applescript
    from .window_resolve import list_windows, resolve_cgwindow_id

    def osa(script) -> str:
        return subprocess.run(
            ["osascript", "-e", script], capture_output=True, text=True
        ).stdout.strip()

    def run_cmd(cmd) -> int:
        return subprocess.run(cmd, capture_output=True).returncode

    home = Path(tempfile.mkdtemp(prefix="clave-dev-guihome-"))
    env_prefix = f"CLAVE_HOME={home} CLAVE_SKIP_ONBOARDING=1 "
    title = f"clave-dev {uuid.uuid4().hex[:8]}"

    win_id = osa(launch_applescript(Path(binary), title, Path(cwd), env_prefix))
    osa(apply_bounds_applescript(profile, win_id or None))
    time.sleep(1.8)  # дать UI подняться

    for keys, wait_s in steps:  # только строки целиком (do script добавляет Return)
        if win_id:
            osa(send_line_applescript(win_id, keys))
        time.sleep(max(0.2, float(wait_s)))
    time.sleep(float(settle_s))

    cgid = resolve_cgwindow_id(list_windows(), profile.app, title)
    out = Path(tempfile.mkdtemp(prefix="clave-dev-shot-")) / "frame.png"
    verdict = run_visual(cgid, vision, run_cmd, _decode_png_pixels, out, prompt)

    if win_id:
        osa(send_line_applescript(win_id, "/quit"))  # выход через tty ЭТОГО окна
        time.sleep(0.8)
        osa(close_window_applescript(win_id))  # и само окно — иначе копятся окна-мусор
    return verdict


def _observe_one(cfg, fresh, scenario):
    from .terminal_profile import default_profile

    profile = default_profile()
    if getattr(cfg, "terminal_profile", None):
        profile = profile._replace(theme=cfg.terminal_profile)
    return gui_capture_verdict(
        fresh,
        cfg.worktree,
        profile,
        cfg.vision,
        steps=scenario.steps,
        settle_s=getattr(scenario, "settle_s", 0.4),
    )


def _decode_png_pixels(path) -> bytes:
    """PNG → сырые пиксели через Quartz (для is_blank_frame). Ленивый импорт pyobjc.
    При любой беде декодирования — пустые байты (→ is_blank_frame=True → блок, не тихий pass)."""
    try:
        import Quartz

        url = Quartz.CFURLCreateWithFileSystemPath(
            None, str(path), Quartz.kCFURLPOSIXPathStyle, False
        )
        src = Quartz.CGImageSourceCreateWithURL(url, None)
        img = Quartz.CGImageSourceCreateImageAtIndex(src, 0, None)
        data = Quartz.CGDataProviderCopyData(Quartz.CGImageGetDataProvider(img))
        return bytes(data)
    except Exception:
        return b""
