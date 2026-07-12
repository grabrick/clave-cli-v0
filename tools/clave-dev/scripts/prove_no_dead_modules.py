#!/usr/bin/env python3
"""Найти модули, которые набор тестов НИ РАЗУ не выполняет.

ПРАВИЛО 3: запускай, а не рассуждай. Ни один модуль не освобождён от тестов.

«Это e2e-only, тестом не покрыть» — самая дорогая фраза в проекте. Именно под ней молча сломался
vision_probe, когда захват стал возвращать список выборок: `probe_summary` полез бы в `.issues` у
списка, и не заметил никто. Освобождение от проверки всегда обосновано и всегда выходит боком:
«тут нечего ломаться», «это просто обёртка», «я же вижу, что верно».

Считаем не импорты, а ВЫПОЛНЕНИЕ: импортированный, но ни разу не вызванный модуль ничем не
проверен. Отсюда трассировка, а не разбор `sys.modules`.

Выход: 0 — все модули выполняются, 1 — есть мёртвые.
"""
from __future__ import annotations

import io
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))

# Мета-тесты сами гоняют набор в подпроцессах — трассировкой их не поймать, и они не про модули.
SKIP_TESTS = ("test_gates_can_fail", "test_no_dead_modules", "test_rules_are_enforced")

# Точки входа: у них нет и не может быть юнит-теста, они лишь склеивают остальное.
ENTRYPOINTS = {"__init__", "__main__"}


def executed_modules() -> set:
    seen = set()

    def profiler(frame, event, arg):
        # setprofile, а не settrace: он срабатывает только на call/return, а не на каждой
        # строке — набор с ним идёт втрое быстрее. Дорогое правило отключат, и оно снова
        # станет советом.
        if event == "call":
            name = frame.f_globals.get("__name__", "")
            if name.startswith("clave_dev."):
                seen.add(name.split(".", 1)[1])

    loader = unittest.TestLoader()
    suite = unittest.TestSuite(
        s
        for s in loader.discover(str(ROOT / "tests"), top_level_dir=str(ROOT))
        if not any(skip in str(s) for skip in SKIP_TESTS)
    )
    sys.setprofile(profiler)
    try:
        unittest.TextTestRunner(stream=io.StringIO(), verbosity=0).run(suite)
    finally:
        sys.setprofile(None)
    return seen


def all_modules() -> set:
    return {
        p.stem
        for p in (ROOT / "clave_dev").glob("*.py")
        if p.stem not in ENTRYPOINTS
    }


def main() -> int:
    dead = sorted(all_modules() - executed_modules())
    if not dead:
        print(f"OK  все {len(all_modules())} модулей выполняются набором")
        return 0
    print("ПРОВАЛ  набор ни разу не выполняет эти модули:", file=sys.stderr)
    for name in dead:
        print(f"  · clave_dev/{name}.py", file=sys.stderr)
    print("\n«e2e-only, тестом не покрыть» — так молча ломается код.", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
