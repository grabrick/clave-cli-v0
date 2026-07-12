#!/usr/bin/env python3
"""Обезвредить один гейт и прогнать набор тестов. Красный набор = гейт доказан.

Правило: **гейт обязан уметь провалиться**. Проверяется это единственным способом — сломать его
и убедиться, что тесты это заметили. Если гейт заменить на «всегда пропускай», а набор останется
зелёным, значит гейт не проверен ничем и это не гейт, а декорация.

Правило выведено из собственных граблей проекта, и каждая была именно такой:
  * `|| true` в страже prod-ветки проглатывал даже поломку регулярки — страж мог отвечать
    только «OK»;
  * `all([])` равно `True`, поэтому зрение, которое НЕ ОТРАБОТАЛО, читалось как «зрение не
    возражает»;
  * критерий сходимости считал зелёный репозиторий заслугой агента, который не тронул ни строки.

Ни одну из трёх не поймали 86 юнит-тестов: они проверяли, что гейт пропускает хорошее, и ни разу —
что он задерживает плохое.

Патчим ДО импорта тестов: они делают `from clave_dev.loop import converged`, то есть связывают имя
в момент своего импорта. Пропатчишь после — тесты будут держать оригинал, и мутация ничего не
докажет.

Выход: 0 — гейт доказан (набор покраснел), 1 — гейт не проверен ничем.
"""
from __future__ import annotations

import importlib
import io
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))

# Мета-тесты исключаем из мутационного прогона. Не только от рекурсии: они запускают набор в
# ПОДПРОЦЕССЕ, а подпроцесс не видит подменённый гейт — то есть доказывают ноль, а стоят по
# полному прогону каждый. Восемь гейтов × вложенный прогон и давали 22с на ровном месте.
META_TESTS = ("test_gates_can_fail", "test_no_dead_modules")


def _always_pass_verdict(samples, base=(), min_hits=2):
    from clave_dev.visual_verdict import ChecklistItem, VisionVerdict

    return VisionVerdict(
        issues=[],
        checklist=[ChecklistItem(check="ok", required=True, passed=True, note="")],
        open_critique="",
        raw="",
    )


# Как обезвредить каждый гейт: подменить его тем, что НИКОГДА не задерживает плохое.
NEUTERED = {
    # «Продукт здоров» — всегда.
    "clave_dev.loop:converged": lambda *a, **k: True,
    # «Задача выполнена» — всегда.
    "clave_dev.loop:outcome": lambda *a, **k: "converged",
    # «Вердикт зрения пропускает» — всегда.
    "clave_dev.visual_verdict:verdict_passes": lambda *a, **k: True,
    # Консенсус выборок — всегда чистый.
    "clave_dev.visual_verdict:consensus_verdict": _always_pass_verdict,
    # Поломка визуального прохода — «её не бывает».
    "clave_dev.visual_verdict:is_infra_failure": lambda v: False,
    # Assertions над экраном — ничего не проверяем.
    "clave_dev.assertions:evaluate": lambda assertions, grid, exit_code: [],
    # «Падений тестов нет» — всегда.
    "clave_dev.checks:parse_test_failures": lambda output: 0,
    # «Агент что-то изменил» — всегда (тогда no-op снова читается как сходимость).
    "clave_dev.diff:changed_paths": lambda worktree: ["src/подменено.rs"],
}


def run_suite_without_self() -> unittest.TestResult:
    loader = unittest.TestLoader()
    suite = unittest.TestSuite(
        s
        for s in loader.discover(str(ROOT / "tests"), top_level_dir=str(ROOT))
        if not any(meta in str(s) for meta in META_TESTS)
    )
    return unittest.TextTestRunner(stream=io.StringIO(), verbosity=0).run(suite)


def main(argv=None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    if len(argv) != 1 or argv[0] not in NEUTERED:
        print(f"использование: prove_gate.py <{'|'.join(NEUTERED)}>", file=sys.stderr)
        return 2

    target = argv[0]
    module_name, _, attr = target.partition(":")
    module = importlib.import_module(module_name)
    if not hasattr(module, attr):
        print(f"нет такого гейта: {target}", file=sys.stderr)
        return 2

    setattr(module, attr, NEUTERED[target])  # ДО импорта тестов
    result = run_suite_without_self()

    caught = len(result.failures) + len(result.errors)
    if caught:
        print(f"OK  {target}: обезврежен → набор покраснел ({caught} тестов)")
        return 0
    print(f"ПРОВАЛ  {target}: обезврежен, а набор ЗЕЛЁНЫЙ — гейт не проверен ничем", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
