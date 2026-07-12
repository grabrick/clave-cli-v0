"""Слой 'Terminal window → CGWindowID' (спека §5). AppleScript id ≠ CGWindowID."""
from __future__ import annotations


class WindowNotFoundError(RuntimeError):
    pass


def resolve_cgwindow_id(window_infos: list, owner: str, title: str) -> int:
    """Чистая логика: из списка window-info (ключи Quartz kCGWindow*) выбрать окно
    владельца `owner` с титулом, содержащим `title`. 0 или >1 совпадений → ошибка
    с перечислением кандидатов (не угадываем)."""
    matches = [
        w for w in window_infos
        if w.get("kCGWindowOwnerName") == owner and title in (w.get("kCGWindowName") or "")
    ]
    if len(matches) == 1:
        return int(matches[0]["kCGWindowNumber"])
    candidates = [(w.get("kCGWindowOwnerName"), w.get("kCGWindowName")) for w in window_infos]
    raise WindowNotFoundError(
        f"ожидалось ровно одно окно {owner!r} с титулом ~{title!r}, найдено {len(matches)}; "
        f"кандидаты: {candidates}"
    )


def list_windows() -> list:
    """Тонкая обёртка над Quartz (ленивый импорт: чистая логика тестируется без pyobjc)."""
    import Quartz  # pyobjc-framework-Quartz

    infos = Quartz.CGWindowListCopyWindowInfo(
        Quartz.kCGWindowListOptionOnScreenOnly, Quartz.kCGNullWindowID
    )
    return list(infos or [])
