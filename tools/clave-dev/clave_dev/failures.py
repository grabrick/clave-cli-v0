"""Суть провала проверки: ЧТО упало, а не сколько.

Человек в терминале видел `✗ test — 1 failed` — и всё. Какой тест упал и почему, уезжало ТОЛЬКО в
промпт агента через build_context. Я сам на этом застрял посреди живого прогона и полез читать
worktree руками.

Это ровно та слепота, которую вчера починили у зрения (перечень забракованного поехал в событие
рядом со счётчиками) — просто у проверок её не заметили. Гейт, который говорит «нет», не объясняя
почему, заставляет человека лезть в потроха; а человек, которому лень лезть, начнёт гейт
отключать.

Весь лог cargo в терминал не влить — поэтому вытаскиваем суть: имена упавших тестов, строки ошибок
clippy, файлы, которые не отформатированы.
"""
from __future__ import annotations

import re

# cargo: «test app::footer::tests::x ... FAILED»
_CARGO_TEST_FAIL = re.compile(r"^test (\S+) \.\.\. FAILED", re.M)
# cargo: «thread '…' panicked at src/x.rs:12:5:»  + следующая строка — причина
_PANIC = re.compile(r"^thread '.*?' .*?panicked at (\S+):\n(.+)$", re.M)
# cargo/clippy: «error: …» и «error[E0308]: …»
_ERROR = re.compile(r"^(error(?:\[\w+\])?: .+)$", re.M)
# rustfmt: «Diff in /path/to/file.rs:12:»
_FMT_DIFF = re.compile(r"^Diff in (\S+?)(?::\d+)?:", re.M)
# unittest: «FAIL: test_x (tests.test_y.Z)» / «ERROR: …»
_PY_FAIL = re.compile(r"^(?:FAIL|ERROR): (\S+) \((\S+?)[.)]", re.M)


def _dedup(items) -> list:
    seen, out = set(), []
    for item in items:
        if item not in seen:
            seen.add(item)
            out.append(item)
    return out


def failure_lines(name: str, raw: str, limit: int = 6) -> list:
    """Что именно провалилось в проверке `name`. Пусто — если разобрать нечего.

    `limit` есть, потому что сотня упавших тестов в терминале нечитаема, а первые несколько
    объясняют причину. Урезание НЕ молчаливое: вызывающий дописывает «… и ещё N».
    """
    raw = raw or ""
    if name == "test":
        failed = _dedup(_CARGO_TEST_FAIL.findall(raw))
        panics = _dedup(f"{where} — {why.strip()}" for where, why in _PANIC.findall(raw))
        return (failed + panics)[:limit]
    if name == "python":
        return _dedup(f"{test} ({mod})" for test, mod in _PY_FAIL.findall(raw))[:limit]
    if name in ("clippy", "build"):
        return _dedup(_ERROR.findall(raw))[:limit]
    if name == "fmt":
        return _dedup(f"не отформатирован: {path}" for path in _FMT_DIFF.findall(raw))[:limit]
    return []


def failure_payload(name: str, raw: str, limit: int = 6) -> dict:
    """Поля события `check`, объясняющие провал. Урезание объявляется вслух."""
    lines = failure_lines(name, raw, limit)
    total = len(failure_lines(name, raw, limit=10_000))
    payload = {"failures": lines}
    if total > len(lines):
        payload["failures_truncated"] = total - len(lines)
    return payload
