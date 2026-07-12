"""Запуск clave в Terminal.app и отправка клавиш через AppleScript (спека §2, §5)."""
from __future__ import annotations

from pathlib import Path


def launch_applescript(binary: Path, title: str, cwd: Path) -> str:
    """Открыть новое окно Terminal, задать уникальный титул (для разрешения окна) и
    запустить clave в нужном каталоге."""
    cmd = f"cd {cwd}; clear; printf '\\\\033]0;{title}\\\\007'; {binary}"
    return (
        'tell application "Terminal"\n'
        "  activate\n"
        f'  do script "{cmd}"\n'
        f'  set custom title of front window to "{title}"\n'
        "end tell"
    )


def keystroke_applescript(keys: str) -> str:
    """Отправить строку клавиш активному окну через System Events (нужен Accessibility)."""
    escaped = keys.replace("\\", "\\\\").replace('"', '\\"')
    return f'tell application "System Events" to keystroke "{escaped}"'
