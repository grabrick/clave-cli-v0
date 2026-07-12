"""Снимок конкретного окна и детект пустого кадра (нет Screen Recording) — спека §5, §8."""
from __future__ import annotations

from pathlib import Path


class ScreenPermissionError(RuntimeError):
    pass


def screencapture_cmd(cgwindow_id: int, out_path: Path) -> list:
    """-x без звука, -o без тени окна, -l<id> конкретное окно по CGWindowID."""
    return ["screencapture", "-x", "-o", f"-l{cgwindow_id}", str(out_path)]


def is_blank_frame(pixels: bytes, threshold: float = 0.02) -> bool:
    """Доля не-нулевых байтов ниже threshold → кадр практически пустой/чёрный
    (типичный признак отсутствия разрешения на запись экрана). Пустой ввод → пустой кадр."""
    if not pixels:
        return True
    nonzero = sum(1 for b in pixels if b != 0)
    return (nonzero / len(pixels)) < threshold
