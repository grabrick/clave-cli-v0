"""Визуальный проход: снять окно Terminal и получить вердикт зрения (спека §7, §8).
Ядро (`run_visual`) тестируется здесь через инъекцию `run_cmd`/`read_pixels` без GUI;
любая беда захвата/парсинга → блокирующий вердикт, никогда тихий pass (§8)."""
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
