"""Управление окном Terminal.app средствами САМОГО Terminal — без глобальной инъекции ввода.

Раньше клавиши слались через System Events `keystroke`. Это ГЛОБАЛЬНАЯ инъекция: символы
уходят тому окну, что сейчас фронтовое. В фоновом прогоне `/dev` (минуты работы) это может
прилететь в чужое приложение или в собственное окно пользователя — недопустимо делать за его
спиной. У Terminal.app есть `do script … in window id X`, который пишет строго в tty нужного
окна. Побочный выигрыш: разрешение Accessibility больше не требуется, остаётся только
Screen Recording.
"""
from __future__ import annotations

from pathlib import Path


def launch_applescript(binary: Path, title: str, cwd: Path, env_prefix: str = "") -> str:
    """Открыть окно Terminal, запустить бинарь, вернуть **id окна Terminal**.

    Окно ищем по ВКЛАДКЕ, которую вернул `do script`, а не по «front window». Проверено
    вживую: без `activate`, когда у пользователя уже открыты окна Terminal (тем более на
    разных Space), «фронтовое» — вовсе не то, что мы только что создали. Раньше это
    маскировал `activate`; убрав его, мы бы вешали титул на чужое окно, а своё теряли.

    Без `activate`: прогон фоновый, воровать фокус у пользователя нельзя.
    `env_prefix` задаёт окружение прямо в команде — иначе наблюдаемый clave читал бы
    РЕАЛЬНЫЙ конфиг и чаты пользователя вместо изолированных."""
    cmd = f"cd {cwd}; clear; {env_prefix}{binary}"
    # Титул вешаем прямо на ВКЛАДКУ, которую вернул `do script`, а окно находим по её
    # уникальному `tty`. Два тупика, проверенных вживую: «front window» без `activate`
    # указывает на чужое окно, а `tabs of w contains t` падает с -1700 (AppleScript не
    # умеет сравнивать ссылки на вкладки). tty уникален и сравнивается как строка.
    return (
        'tell application "Terminal"\n'
        f'  set t to do script "{cmd}"\n'
        f'  set custom title of t to "{title}"\n'
        "  set theTty to tty of t\n"
        "  repeat with w in windows\n"
        "    repeat with tb in tabs of w\n"
        "      if tty of tb is theTty then return id of w\n"
        "    end repeat\n"
        "  end repeat\n"
        "end tell"
    )


def close_window_applescript(window_id) -> str:
    """Закрыть окно Terminal целиком.

    Обязательно: после `/quit` сам clave выходит, но окно ОСТАЁТСЯ — в нём возвращается
    shell. Без явного закрытия каждый визуальный проход оставлял бы висящее окно, и за три
    раунда `/dev` пользователь получал бы три окна-мусора."""
    return f'tell application "Terminal" to close (every window whose id is {window_id})'


def send_line_applescript(window_id, text: str) -> str:
    """Написать строку в tty КОНКРЕТНОГО окна Terminal (текст + Return).

    Ограничение честное: так шлётся только строка целиком (с Enter). Одиночные клавиши
    без Enter (например `?`) этим безопасным путём не отправить — для v1 достаточно, так как
    визуальный проход снимает стартовый экран (именно он и вскрыл баг среза футера)."""
    escaped = text.replace("\\", "\\\\").replace('"', '\\"')
    return f'tell application "Terminal" to do script "{escaped}" in window id {window_id}'
