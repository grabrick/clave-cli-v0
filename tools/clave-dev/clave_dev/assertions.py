"""Машинно-проверяемые предикаты над символьной сеткой (список строк)."""
from __future__ import annotations

import re
from collections import namedtuple

AssertionResult = namedtuple("AssertionResult", "name passed message")


def visible(sub: str):
    def check(grid, exit_code):
        ok = any(sub in row for row in grid)
        return AssertionResult(f"visible({sub!r})", ok, "" if ok else f"не найдено: {sub!r}")

    return check


def not_visible(sub: str):
    def check(grid, exit_code):
        ok = all(sub not in row for row in grid)
        return AssertionResult(
            f"not_visible({sub!r})", ok, "" if ok else f"найдено, но не должно: {sub!r}"
        )

    return check


def line_matches(pattern: str):
    rx = re.compile(pattern)

    def check(grid, exit_code):
        ok = any(rx.search(row) for row in grid)
        return AssertionResult(f"line_matches({pattern!r})", ok, "" if ok else "нет строки под шаблон")

    return check


def no_line_overflows_width(width: int):
    def check(grid, exit_code):
        bad = next((r for r in grid if len(r.rstrip()) > width), None)
        ok = bad is None
        return AssertionResult("no_line_overflows_width", ok, "" if ok else f"строка шире {width}: {bad!r}")

    return check


def launched():
    def check(grid, exit_code):
        ok = any(row.strip() for row in grid)
        return AssertionResult("launched", ok, "" if ok else "экран пуст")

    return check


def clean_exit():
    def check(grid, exit_code):
        ok = exit_code == 0
        return AssertionResult("clean_exit", ok, "" if ok else f"exit={exit_code}")

    return check


def structural_assertions():
    """Базовые assertions, активные всегда (спека §5)."""
    return [launched(), clean_exit()]


def evaluate(assertions, grid, exit_code):
    return [a(grid, exit_code) for a in assertions]
