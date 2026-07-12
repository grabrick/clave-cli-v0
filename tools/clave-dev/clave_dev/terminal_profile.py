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


def settings_set_exists(name: str) -> bool:
    """Есть ли у Terminal профиль с таким именем.

    Он обязателен: только профиль с «при выходе из shell — закрыть окно» позволяет окну
    визуального прохода убрать себя. Снаружи окно Terminal не закрывается (см.
    terminal_driver.launch_applescript). Свежесозданный профиль работающий Terminal НЕ
    видит — его нужно перезапустить; поэтому проверку делаем в preflight, до прогона."""
    import subprocess

    out = subprocess.run(
        ["osascript", "-e", 'tell application "Terminal" to return name of every settings set'],
        capture_output=True,
        text=True,
    ).stdout
    return name in out


def describe(p: TerminalProfile) -> dict:
    """Плоский dict для лога/отчёта — атрибуция любого визуального вывода к среде."""
    return dict(p._asdict())


def apply_bounds_applescript(p: TerminalProfile, window_id=None) -> str:
    """AppleScript: выставить bounds окна Terminal (цель §4 — детерминизм среды).
    Если известен id окна — целимся в него, а не в «фронтовое» (надёжнее и не зависит
    от того, что пользователь успел кликнуть)."""
    x, y, w, h = p.bounds
    target = f"window id {window_id}" if window_id else "front window"
    return (
        f'tell application "Terminal" to set bounds of {target} '
        f"to {{{x}, {y}, {x + w}, {y + h}}}"
    )
