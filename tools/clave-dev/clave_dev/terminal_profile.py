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


def _term_prop(target: str, prop: str) -> str:
    import subprocess

    out = subprocess.run(
        ["osascript", "-e", f'tell application "Terminal" to return ({prop} of {target}) as text'],
        capture_output=True,
        text=True,
    )
    return out.stdout.strip()


def observer_profile_mismatch(name: str, get_prop=None):
    """Расхождение профиля НАБЛЮДАТЕЛЯ с рабочим профилем пользователя (None — совпадают).

    Это не косметика. Класс багов, ради которого построено зрение (срез у правой стенки,
    обрезанные глифы), зависит от ШРИФТА и ширин глифов. Если окно наблюдателя выглядит
    иначе, чем окно пользователя, зрение судит рендер, которого пользователь никогда не
    видит — то есть молча подменяет предмет проверки. Профиль наблюдателя обязан быть
    копией рабочего, отличаясь ровно одним: «при выходе из shell — закрыть окно»."""
    prop = get_prop or _term_prop
    default_name = prop("default settings", "name")
    if not default_name or default_name == name:
        return None
    diffs = [
        p
        for p in ("background color", "font name")
        if (a := prop("default settings", p)) and (b := prop(f'settings set "{name}"', p)) and a != b
    ]
    if not diffs:
        return None
    return (
        f"профиль наблюдателя «{name}» не совпадает с твоим рабочим «{default_name}» "
        f"({', '.join(diffs)}). Зрение будет судить рендер, которого ты не видишь, — "
        f"а ловимые баги зависят от шрифта. Сделай «{name}» копией «{default_name}», "
        "поменяв только «при выходе из shell — закрыть окно», и перезапусти Terminal"
    )


def describe(p: TerminalProfile) -> dict:
    """Плоский dict для лога/отчёта — атрибуция любого визуального вывода к среде."""
    return dict(p._asdict())


def apply_geometry_applescript(p: TerminalProfile, window_id=None) -> str:
    """AppleScript: задать положение окна в пикселях, а размер — В ЗНАКОМЕСТАХ.

    Пиксельных bounds не хватает. Пересчёт пикселей в колонки делает сам Terminal, и результат
    зависит от того, успело ли окно открыться: в одном прогоне замерено 123×39 на базовой
    сборке и 120×30 на свежей. А чеклист зрения весь про ширину — «текст не касается правой
    границы», «нет обрезанных глифов». Сравнивать рендеры разной ширины бессмысленно: фреш
    поуже дал бы фантомную регрессию на ровном месте. Поэтому колонки и строки задаём прямо.

    Если известен id окна — целимся в него, а не в «фронтовое»: так не зависим от того, что
    пользователь успел кликнуть.
    """
    x, y, w, h = p.bounds
    target = f"window id {window_id}" if window_id else "front window"
    return (
        'tell application "Terminal"\n'
        f"  set bounds of {target} to {{{x}, {y}, {x + w}, {y + h}}}\n"
        f"  set number of columns of {target} to {p.cols}\n"
        f"  set number of rows of {target} to {p.rows}\n"
        "end tell"
    )


def read_geometry_applescript(window_id) -> str:
    """AppleScript: фактическая геометрия окна, «<колонок>x<строк>».

    Задать мало — надо убедиться, что задалось: иначе зрение вынесет вердикт о рендере,
    которого мы не заказывали, и сравнивать его будет не с чем.
    """
    return (
        'tell application "Terminal"\n'
        f"  set c to number of columns of window id {window_id}\n"
        f"  set r to number of rows of window id {window_id}\n"
        "  return (c as text) & \"x\" & (r as text)\n"
        "end tell"
    )


def geometry_label(p: TerminalProfile) -> str:
    return f"{p.cols}x{p.rows}"
