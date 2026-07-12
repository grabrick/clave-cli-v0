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


def launch_applescript(
    binary: Path, title: str, cwd: Path, env_prefix: str = "", settings_set: str = None
) -> str:
    """Открыть окно Terminal, запустить бинарь, вернуть **id окна Terminal**.

    Команда заканчивается `; exit`, а вкладке назначается профиль `settings_set`, у которого
    «при выходе из shell — закрыть окно». Тогда после `/quit` clave выходит, следом выходит
    шелл, и окно закрывается САМО.

    Почему только так. Закрыть окно Terminal снаружи нельзя — проверено на живой системе:
    AppleScript `close` молча не срабатывает (возвращает успех, окно живо), `close` вкладки
    Terminal не понимает (-1708), а System Events на ВВОД (и keystroke, и клик по крестику)
    из нашего контекста не действует вовсе. Из-за этого прежний teardown через
    `keystroke "/quit"` не работал НИКОГДА, и окна копились после каждого прогона.

    Окно ищем по ВКЛАДКЕ, которую вернул `do script`, а не по «front window». Проверено
    вживую: без `activate`, когда у пользователя уже открыты окна Terminal (тем более на
    разных Space), «фронтовое» — вовсе не то, что мы только что создали. Раньше это
    маскировал `activate`; убрав его, мы бы вешали титул на чужое окно, а своё теряли.

    Без `activate`: прогон фоновый, воровать фокус у пользователя нельзя.
    `env_prefix` задаёт окружение прямо в команде — иначе наблюдаемый clave читал бы
    РЕАЛЬНЫЙ конфиг и чаты пользователя вместо изолированных."""
    cmd = f"cd {cwd}; clear; {env_prefix}{binary}; exit"
    # Титул вешаем прямо на ВКЛАДКУ, которую вернул `do script`, а окно находим по её
    # уникальному `tty`. Два тупика, проверенных вживую: «front window» без `activate`
    # указывает на чужое окно, а `tabs of w contains t` падает с -1700 (AppleScript не
    # умеет сравнивать ссылки на вкладки). tty уникален и сравнивается как строка.
    lines = ['tell application "Terminal"', f'  set t to do script "{cmd}"']
    if settings_set:
        lines.append(f'  set current settings of t to settings set "{settings_set}"')
    lines += [
        f'  set custom title of t to "{title}"',
        "  set theTty to tty of t",
        "  repeat with w in windows",
        "    repeat with tb in tabs of w",
        "      if tty of tb is theTty then return id of w",
        "    end repeat",
        "  end repeat",
        "end tell",
    ]
    return "\n".join(lines)


def send_line_applescript(window_id, text: str) -> str:
    """Написать строку в tty КОНКРЕТНОГО окна Terminal (текст + Return).

    Ограничение честное: так шлётся только строка целиком (с Enter). Одиночные клавиши
    без Enter (например `?`) этим безопасным путём не отправить — для v1 достаточно, так как
    визуальный проход снимает стартовый экран (именно он и вскрыл баг среза футера)."""
    escaped = text.replace("\\", "\\\\").replace('"', '\\"')
    return f'tell application "Terminal" to do script "{escaped}" in window id {window_id}'


def tty_of_window_applescript(window_id) -> str:
    """tty вкладки окна — по нему гасятся её процессы."""
    return f'tell application "Terminal" to get tty of (first tab of window id {window_id})'


def close_window_applescript(title: str) -> str:
    """Закрыть наше окно по уникальному титулу.

    Форма важна. Поштучный `close w` внутри `repeat` рапортует успех и НЕ ЗАКРЫВАЕТ НИЧЕГО —
    проверено на живой системе, окна оставались висеть. А `close (every window whose …)`
    работает. Прежний вывод «закрыть окно Terminal снаружи нельзя» был сделан по первой форме
    и оказался неверным — а на нём стояла вся схема «окно закроет себя само», из-за которой
    после прогонов натекло 24 окна, в каждом живой clave.

    Титул уникален (nonce), так что чужие окна пользователя под условие не попадут.
    """
    safe = title.replace("\\", "\\\\").replace('"', '\\"')
    return (
        'tell application "Terminal" to close '
        f'(every window whose name contains "{safe}") saving no'
    )
