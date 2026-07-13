"""Визуальный проход: снять окно Terminal и получить вердикт зрения (спека §7, §8).
Ядро (`run_visual`) тестируется здесь через инъекцию `run_cmd`/`read_pixels` без GUI.
GUI-оркестрация (`observe_visual_all`) — e2e-only (реальный Terminal.app), обёрнута в
fail-safe: любая беда → блокирующий вердикт, никогда тихий pass (§8)."""
from __future__ import annotations

import os
import subprocess
import sys
import time
from pathlib import Path

from .capture import is_blank_frame, screencapture_cmd
from .visual_verdict import blocking_verdict


def kill_observed_process(tty: str) -> bool:
    """Погасить наблюдаемый бинарь во вкладке — и ТОЛЬКО его. True — если было кого гасить.

    Почему жёстко, а не «попроси приложение выйти». Прежний teardown слал в окно `/quit` и
    надеялся. Проверено на живой системе: clave от этого НЕ выходит — три процесса во вкладке до
    и три после. Полагаться тут на продукт нельзя и по существу: агент мог сломать его ровно так,
    что он не реагирует на ввод, — а это и есть тот случай, ради которого визуальный проход
    существует.

    Почему только бинарь, не весь tty. Логин-шелл трогать нельзя: команда запуска кончается на
    `; exit`, так что стоит убить бинарь — и шелл сам доигрывает `exit` и выходит ЧИСТО, а
    Terminal убирает вкладку и окно. Если же прибить и шелл, вкладка разрушается криво: остаётся
    «зомби»-окно без вкладок, которое AppleScript уже не закрывает ничем. Так я и насорил
    двумя десятками окон.

    Не через `pkill -t`: на macOS он ТРЕБУЕТ pattern и без него молча падает с кодом 2.
    """
    dev = str(tty).rsplit("/", 1)[-1]
    if not dev.startswith("ttys"):
        return False
    listing = subprocess.run(
        ["ps", "-t", dev, "-o", "pid=,command="], capture_output=True, text=True, check=False
    )
    victims = []
    for line in listing.stdout.splitlines():
        pid, _, command = line.strip().partition(" ")
        command = command.strip()
        # логин-шелл начинается с дефиса (`-zsh`), сам login — с `login`. Всё прочее — наше.
        if command.startswith("-") or command.startswith("login"):
            continue
        if pid.isdigit():
            victims.append(pid)
    if not victims:
        return False
    subprocess.run(["kill", "-9", *victims], capture_output=True, check=False)
    return True


def tab_is_empty(tty: str) -> bool:
    """В tty вкладки не осталось ни одного процесса (включая root-овый `login`)."""
    dev = str(tty).rsplit("/", 1)[-1]
    if not dev.startswith("ttys"):
        return True
    res = subprocess.run(["ps", "-t", dev, "-o", "pid="], capture_output=True, text=True, check=False)
    return not res.stdout.strip()


def window_still_open(osa, title: str) -> bool:
    """Осталось ли НАШЕ окно висеть на экране.

    Считаем окно живым, только если у него ЕСТЬ вкладка. Закрытое окно Terminal ещё какое-то
    время числится в списке `windows` — но уже с нулём вкладок. Проверка по одному имени принимала
    такой фантом за протечку и подняла три ложные тревоги подряд, пока я не сверился со списком
    вкладок.

    Ищем по имени окна, а не по `custom title` вкладки: Terminal стирает custom title, как только
    процесс во вкладке умирает, — по нему настоящая протечка была бы, наоборот, невидима.

    Титул уникален (`clave-dev <nonce>`), и это принципиально: фильтровать по одному лишь
    «clave-dev» нельзя — под такую подстроку попадёт и окно пользователя, открытое в каталоге
    вроде `…/worktrees/clave-dev-headless`, и мы закроем ему рабочее окно.
    """
    safe = title.replace('"', '\\"')
    script = (
        'tell application "Terminal"\n'
        "  set n to 0\n"
        f'  repeat with w in (every window whose name contains "{safe}")\n'
        "    try\n"
        "      if (count of tabs of w) > 0 then set n to n + 1\n"
        "    end try\n"
        "  end repeat\n"
        "  return n\n"
        "end tell"
    )
    try:
        return int(osa(script) or "0") > 0
    except (TypeError, ValueError):
        return False


def teardown_window(osa, title: str, tty: str, close_script) -> bool:
    """Убрать наше окно. True — убрано.

    Закрывает окно САМ Terminal — по настройке профиля «при выходе из shell закрыть окно»
    (`shellExitAction = 0`). Мы лишь гасим наблюдаемый бинарь: шелл доигрывает `; exit`, выходит
    чисто, и окно уходит вместе со вкладкой.

    Почему не закрываем через AppleScript. Проверено на живой системе: `close` на окне Terminal
    возвращает успех и НЕ ЗАКРЫВАЕТ — ни по id, ни через `whose`, ни с `saving no`. Попытку всё
    же делаем (вдруг повезёт), но рассчитывать на неё нельзя.

    ВАЖНО: Terminal читает профиль из настроек ОДИН раз, при своём запуске. Если профиль правили
    при живом Terminal, изменение подхватится только после его перезапуска — до тех пор окна
    будут копиться, и про каждое такое мы честно предупредим.
    """
    kill_observed_process(tty)
    for _ in range(20):  # до ~5 с: шелл доигрывает `exit`, Terminal убирает вкладку
        if tab_is_empty(tty):
            break
        time.sleep(0.25)

    for _ in range(3):
        if not window_still_open(osa, title):
            return True
        osa(close_script)  # запасная попытка; на живой системе она обычно не срабатывает
        time.sleep(0.5)
    return not window_still_open(osa, title)


def run_visual(
    cgwindow_id, vision, run_cmd, read_pixels, out_path: Path, prompt=None, samples: int = 1
) -> list:
    """Снять окно `cgwindow_id` и оценить зрением `samples` раз. `run_cmd(list)->int`,
    `read_pixels(path)->bytes` инъектируются (в проде — subprocess/декод PNG; в тестах — фейки).

    Кадр снимается ОДИН раз, а судится несколько: замеренный разброс даёт судья, а не скриншот
    (пять прогонов на неизменном продукте — три разных вердикта). Пересъёмка окна ради выборок
    была бы дороже и мешала бы шум судьи с дрожанием курсора.

    Две беды тут РАЗНОЙ природы, и путать их нельзя:

    * Поломка ЗАХВАТА (нет Screen Recording, чёрный кадр) — фатальна: судить нечего, все выборки
      обречены. Блокируем сразу.
    * Осечка СУДЬИ (модель вернула битый JSON) — это та же самая ненадёжность, ради которой мы и
      берём несколько выборок. Живой прогон споткнулся ровно об это: одна испорченная выборка из
      трёх убила базовую линию, а с ней и двадцать минут работы агента. Осечку просто повторяем;
      блокируем, только если не разобрана НИ ОДНА выборка — вот тогда зрение и правда мертво.

    Возвращает список вердиктов (при поломке — один блокирующий).
    """
    from .vision import DEFAULT_VISION_PROMPT

    code = run_cmd(screencapture_cmd(cgwindow_id, out_path))
    if code != 0:
        return [blocking_verdict(f"screencapture код {code} (нет Screen Recording?)")]
    if is_blank_frame(read_pixels(out_path)):
        return [blocking_verdict("кадр пустой/чёрный — вероятно нет разрешения на запись экрана")]

    wanted = max(1, samples)
    verdicts, failures = [], []
    for _ in range(wanted * 2):  # запас на осечки судьи
        if len(verdicts) >= wanted:
            break
        try:
            verdicts.append(vision.analyze_image(out_path, prompt or DEFAULT_VISION_PROMPT))
        except Exception as e:
            failures.append(str(e))

    if not verdicts:
        return [blocking_verdict(f"vision-бэкенд: {'; '.join(failures[:2])}")]
    return verdicts


# --- ниже e2e-only: реальный Terminal.app. Не вызывается в headless-тестах/мок-смоуке ---
# (run_loop гардирует вызов через cfg.vision). Каждый сценарий обёрнут в fail-safe.


def observe_visual_all(cfg, fresh, samples: int = 1):
    """Для каждого сценария поднять fresh в Terminal.app, снять окно, оценить зрением `samples`
    раз. Любая беда сценария → блокирующий вердикт (§8), петля не падает.

    Возвращает список ПО СЦЕНАРИЯМ, где каждый элемент — список выборок вердикта.
    """
    per_scenario = []
    for scenario in cfg.scenarios:
        try:
            per_scenario.append(_observe_one(cfg, fresh, scenario, samples))
        except Exception as e:
            per_scenario.append([blocking_verdict(f"визуальный проход упал: {e}")])
    return per_scenario


def gui_capture_verdict(
    binary, cwd, profile, vision, steps=(), settle_s=0.4, prompt=None, samples: int = 1,
    config_path=None,
):
    """Единственный GUI-проход: поднять бинарь в окне Terminal.app, снять окно, оценить зрением.

    Безопасность (то, ради чего это переписано):
    * НИКАКИХ System Events — общение с окном только через `do script … in window id`,
      то есть в tty конкретного окна. Глобальная инъекция клавиш в фоновом прогоне могла
      прилететь в чужое приложение. Заодно Accessibility больше не нужен.
    * Изолированный CLAVE_HOME — иначе наблюдаемый бинарь лез бы в реальные конфиг и чаты
      пользователя. Плюс детерминизм: свежий home → всегда одинаковый стартовый экран.
    * Без `activate` — фокус у пользователя не воруем.

    `config_path` (CLAVE_CONFIG) обязателен для честного вердикта: изолированный home означает
    ДЕФОЛТНУЮ тему, и зрение судило бы рендер, которого пользователь никогда не видит. Состояние
    остаётся изолированным — перекрывается только путь к конфигу.
    """
    import shutil
    import subprocess
    import tempfile
    import time
    import uuid

    from .terminal_driver import (
        close_window_applescript,
        launch_applescript,
        send_line_applescript,
        tty_of_window_applescript,
    )
    from .terminal_profile import (
        apply_geometry_applescript,
        geometry_label,
        read_geometry_applescript,
    )
    from .window_resolve import list_windows, resolve_cgwindow_id

    def osa(script) -> str:
        return subprocess.run(
            ["osascript", "-e", script], capture_output=True, text=True
        ).stdout.strip()

    def run_cmd(cmd) -> int:
        return subprocess.run(cmd, capture_output=True).returncode

    home = Path(tempfile.mkdtemp(prefix="clave-dev-guihome-"))
    # CLAVE_STATIC_RENDER — рендер без стенных часов. Правый слот футера вращается по
    # времени, и два снимка одного и того же кода показывали разные сегменты разной ширины:
    # регрессионный гейт объявлял регрессией то, чего агент не делал.
    env_prefix = f"CLAVE_HOME={home} CLAVE_SKIP_ONBOARDING=1 CLAVE_STATIC_RENDER=1 "
    if config_path:
        env_prefix += f"CLAVE_CONFIG={config_path} "
    title = f"clave-dev {uuid.uuid4().hex[:8]}"

    # profile.theme — имя профиля Terminal (шрифт и цвета как у пользователя).
    # Закрывать окно на «сам выйдет по shellExitAction» больше НЕ рассчитываем: не закрывалось.
    # Уборка — жёсткая, в конце функции.
    win_id = osa(
        launch_applescript(Path(binary), title, Path(cwd), env_prefix, profile.theme)
    )
    # Геометрию задаём ПОСЛЕ того, как окно открылось. Раньше она выставлялась сразу за launch,
    # в гонке с открытием, и порой не применялась вовсе — в одном прогоне база вышла 123×39, а
    # свежая сборка 120×30. Гейт сравнивал рендеры разной ширины, а вся его required-часть про
    # ширину и есть.
    time.sleep(1.2)  # дать окну открыться
    osa(apply_geometry_applescript(profile, win_id or None))
    time.sleep(0.8)  # ресайз + перерисовка TUI по SIGWINCH

    want = geometry_label(profile)
    got = osa(read_geometry_applescript(win_id)) if win_id else ""

    # Объявляем ДО ветвления: на пути «геометрия не сошлась» снимков нет, а убирать за собой в
    # конце всё равно надо — иначе уборка падала бы на NameError ровно в том прогоне, который и
    # так пошёл не так.
    shot_dir = None

    if got == want:
        for keys, wait_s in steps:  # только строки целиком (do script добавляет Return)
            if win_id:
                osa(send_line_applescript(win_id, keys))
            time.sleep(max(0.2, float(wait_s)))
        time.sleep(float(settle_s))

        cgid = resolve_cgwindow_id(list_windows(), profile.app, title)
        shot_dir = Path(tempfile.mkdtemp(prefix="clave-dev-shot-"))
        out = shot_dir / "frame.png"
        verdicts = run_visual(cgid, vision, run_cmd, _decode_png_pixels, out, prompt, samples)
    else:
        # Молча судить не тот рендер нельзя: вердикт был бы о другом окне, а сравнивать его
        # с базовой линией — тем более. Честнее объявить проход несостоявшимся.
        verdicts = [
            blocking_verdict(
                f"геометрия окна {got or '?'} вместо {want} — вердикт был бы о другом рендере"
            )
        ]

    # Убираем за собой САМИ, не спрашивая продукт. Прежний teardown слал «/quit» и надеялся,
    # что clave вежливо выйдет, шелл выйдет следом, а окно закроется само. Не работало: за
    # прогоны натекло 24 окна Terminal, и в каждом остался живой clave. Полагаться тут на
    # продукт нельзя и по существу — агент мог сломать его ровно так, что он не выходит по
    # команде, а это и есть тот случай, ради которого наблюдатель существует.
    if win_id:
        closed = False
        try:
            tty = osa(tty_of_window_applescript(win_id))
            closed = teardown_window(osa, title, tty, close_window_applescript(title))
        except Exception:
            closed = False  # уборка не должна ронять вердикт — но и молчать о протечке нельзя
        if not closed:
            print(
                f"clave-dev: ⚠ окно Terminal «{title}» не закрылось — закрой вручную (Cmd+W)",
                file=sys.stderr,
            )

    # ...и за каталогами тоже. Окна мы убирали, а `mkdtemp` — нет: за прогоны в $TMPDIR натекло
    # 97 каталогов (по одному guihome на проход плюс по одному shot на КАЖДЫЙ снимок). Утечка
    # ровно того же рода, что 24 окна Terminal, только незаметная — на неё не наткнёшься глазами.
    #
    # guihome убираем ТОЛЬКО после того, как наблюдаемый процесс убит: это его CLAVE_HOME, и
    # выдернуть его из-под живого clave значит получить мусор в вердикте вместо рендера.
    for leftover in (home, shot_dir):
        if leftover:
            shutil.rmtree(leftover, ignore_errors=True)
    return verdicts


def _observe_one(cfg, fresh, scenario, samples: int = 1):
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
        samples=samples,
        config_path=cfg.env.get("CLAVE_CONFIG"),
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
