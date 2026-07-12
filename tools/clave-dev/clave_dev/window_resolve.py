"""Слой 'Terminal window → CGWindowID' (спека §5). AppleScript id ≠ CGWindowID."""
from __future__ import annotations


class WindowNotFoundError(RuntimeError):
    pass


def resolve_cgwindow_id(window_infos: list, owner: str, title: str) -> int:
    """Чистая логика: из списка window-info (ключи Quartz kCGWindow*) выбрать окно по
    УНИКАЛЬНОМУ титулу-nonce; владелец — мягкий фильтр.

    Почему титул первичен: AppleScript адресует приложение английским именем ("Terminal"),
    а CGWindowList отдаёт ЛОКАЛИЗОВАННОЕ (`Терминал` на русской macOS) — они законно
    расходятся. Титул с nonce уникален сам по себе, поэтому по нему и матчим; owner лишь
    уточняет выбор, если по титулу нашлось несколько. 0 или >1 → ошибка с кандидатами."""
    by_title = [w for w in window_infos if title in (w.get("kCGWindowName") or "")]
    same_owner = [w for w in by_title if w.get("kCGWindowOwnerName") == owner]
    matches = same_owner or by_title
    if len(matches) == 1:
        return int(matches[0]["kCGWindowNumber"])
    candidates = [(w.get("kCGWindowOwnerName"), w.get("kCGWindowName")) for w in window_infos]
    raise WindowNotFoundError(
        f"ожидалось ровно одно окно с титулом ~{title!r} (владелец ~{owner!r}), "
        f"найдено {len(matches)}; кандидаты: {candidates}"
    )


def list_windows() -> list:
    """Тонкая обёртка над Quartz (ленивый импорт: чистая логика тестируется без pyobjc).

    Берём `kCGWindowListOptionAll`, а НЕ `OnScreenOnly`. Причина проверена вживую: окно
    Terminal, созданное без `activate`, живёт на СВОЁМ Space, а `OnScreenOnly` перечисляет
    только окна текущего Space — окна там просто нет, и резолв падал. При этом
    `screencapture -l<id>` окно с чужого Space снимает прекрасно (macOS держит backing
    store) — значит переключать Space и воровать у пользователя фокус не нужно вовсе.
    Титул-nonce уникален, так что расширение списка неоднозначности не создаёт."""
    import Quartz  # pyobjc-framework-Quartz

    infos = Quartz.CGWindowListCopyWindowInfo(
        Quartz.kCGWindowListOptionAll, Quartz.kCGNullWindowID
    )
    return list(infos or [])
