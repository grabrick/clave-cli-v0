"""Фиксированная терминальная среда, чтобы vision не ловил шум (спека §4)."""
from __future__ import annotations

from collections import namedtuple

TerminalProfile = namedtuple(
    "TerminalProfile", "app cols rows font font_size theme opacity locale bounds"
)


def default_profile() -> TerminalProfile:
    return TerminalProfile(
        app="Terminal",
        cols=100,
        rows=30,
        font="SF Mono",
        font_size=13,
        theme="clave-dev",
        opacity=1.0,
        locale="ru_RU.UTF-8",
        bounds=(100, 100, 900, 640),  # x, y, w, h
    )


def describe(p: TerminalProfile) -> dict:
    """Плоский dict для лога/отчёта — атрибуция любого визуального вывода к среде."""
    return dict(p._asdict())


def apply_bounds_applescript(p: TerminalProfile) -> str:
    """AppleScript: выставить bounds фронтового окна Terminal (цель §4 — детерминизм среды)."""
    x, y, w, h = p.bounds
    return (
        'tell application "Terminal" to set bounds of front window '
        f"to {{{x}, {y}, {x + w}, {y + h}}}"
    )
