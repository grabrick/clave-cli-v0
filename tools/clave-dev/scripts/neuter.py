#!/usr/bin/env python3
"""Обезвредить ОДНУ функцию пакета и прогнать набор. Зелёный набор = функцию не проверяет ничто.

    python3 scripts/neuter.py clave_dev.loop:converged

Родня `prove_gate.py`, и разница принципиальная. `prove_gate` обезвреживает ГЕЙТЫ: список
фиксирован, пишу его я, и заглушка у каждого своя, осмысленная. Здесь цель — ФУНКЦИИ, КОТОРЫЕ
ДОБАВИЛ АГЕНТ: список берётся из диффа, заранее его никто не знает, и заглушка может быть только
универсальной.

Патчим ДО импорта тестов: они делают `from clave_dev.loop import converged`, то есть связывают имя
в момент своего импорта. Пропатчишь после — тесты будут держать оригинал, и мутация не докажет
ничего.

Мета-тесты исключаем: они гоняют набор в ПОДПРОЦЕССЕ, а подпроцесс не видит подменённую функцию —
то есть доказывают ноль, а стоят по полному прогону каждый.

Выход: 0 — набор заметил подмену (функция проверена), 1 — набор зелёный (не проверена ничем),
2 — такой функции нет (её могли переименовать или удалить — это не вина тестов).
"""
from __future__ import annotations

import importlib
import io
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))

META_TESTS = ("test_gates_can_fail", "test_no_dead_modules", "test_rules_are_enforced")


def run_suite() -> unittest.TestResult:
    loader = unittest.TestLoader()
    suite = unittest.TestSuite(
        s
        for s in loader.discover(str(ROOT / "tests"), top_level_dir=str(ROOT))
        if not any(meta in str(s) for meta in META_TESTS)
    )
    return unittest.TextTestRunner(stream=io.StringIO(), verbosity=0).run(suite)


def main(argv=None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    if len(argv) != 1 or ":" not in argv[0]:
        print("использование: neuter.py <модуль>:<функция>", file=sys.stderr)
        return 2

    target = argv[0]
    module_name, _, attr = target.partition(":")
    try:
        module = importlib.import_module(module_name)
    except ImportError as err:
        print(f"нет такого модуля: {module_name} ({err})", file=sys.stderr)
        return 2
    if not hasattr(module, attr):
        print(f"нет такой функции: {target}", file=sys.stderr)
        return 2

    # Единственная универсальная подмена: «функция ничего не возвращает». Тест, который смотрит на
    # результат, это заметит; тест-декорация — нет. Ровно её мы и ищем.
    setattr(module, attr, lambda *a, **k: None)  # ДО импорта тестов
    result = run_suite()

    caught = len(result.failures) + len(result.errors)
    if caught:
        print(f"OK  {target}: обезврежен → набор покраснел ({caught} тестов)")
        return 0
    print(f"ВЫЖИЛ  {target}: обезврежен, а набор ЗЕЛЁНЫЙ — тест не смотрит на результат", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
