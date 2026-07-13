"""Прогон и разбор проверок в worktree: cargo (Rust) + юнит-набор самого clave-dev (Python)."""
from __future__ import annotations

import re
import subprocess
import sys
from collections import namedtuple
from pathlib import Path

from .binaries import build_command

ChecksResult = namedtuple(
    "ChecksResult", "build_ok test_failures clippy_ok fmt_ok raw py_ok", defaults=(True,)
)

_TEST_RESULT = re.compile(r"test result:\s*\w+\.\s*\d+ passed;\s*(\d+) failed")
_RAN = re.compile(r"^Ran (\d+) tests?", re.M)

# Правила самопроверки инструмента. Перечислены ПОИМЁННО, и это принципиально: удалить
# правило через discover можно молча, а по имени — только с ModuleNotFoundError.
RULE_TESTS = (
    "tests.test_gates_can_fail",   # гейт обязан уметь провалиться
    "tests.test_unverified",       # «сошлось» не едет в одиночку
    "tests.test_no_dead_modules",  # ни один модуль не освобождён от тестов
    "tests.test_rules_are_enforced",  # сторож правил: без имени его можно снести молча
)


def parse_test_failures(output: str) -> int:
    """Сумма 'N failed' по всем строкам 'test result:' (или 0, если ни одной нет)."""
    return sum(int(m.group(1)) for m in _TEST_RESULT.finditer(output))


def tests_ran(output: str) -> int:
    """Сколько тестов НА САМОМ ДЕЛЕ прогнано. Коду возврата тут верить нельзя.

    `sys.exit(0)` в любом модуле, который импортирует тест, убивает unittest молча: ноль тестов,
    пустой вывод, КОД ВОЗВРАТА 0. Прогон, который не состоялся, читается как прогон, который
    прошёл, — та же болезнь, что `all([]) == True`, только на уровне процесса.

    Поймано red-team-прогоном, и это был не теоретический риск: `prove_gate.py`, выпотрошенный до
    `sys.exit(0)`, гасил ВЕСЬ набор правил и оставался «зелёным». Замок ловил правку файла, но
    замок — сирена, а не запрет: одна команда `relock_rules.py` её выключает.

    Поэтому гейт больше не спрашивает «упало ли», он спрашивает «а сколько ты прогнал».
    """
    return sum(int(m.group(1)) for m in _RAN.finditer(output))


def _run(worktree: Path, env: dict, args: list, cwd: Path = None) -> subprocess.CompletedProcess:
    return subprocess.run(
        args, cwd=str(cwd or worktree), env=env, capture_output=True, text=True, check=False
    )


def python_suite_dir(worktree: Path):
    """Каталог python-пакета clave-dev внутри worktree, если он там есть."""
    pkg = Path(worktree) / "tools" / "clave-dev"
    return pkg if (pkg / "clave_dev").is_dir() else None


def run_checks(worktree: Path, env: dict, profile: str) -> ChecksResult:
    raw = {}
    build = _run(worktree, env, build_command(profile))
    raw["build"] = build.stdout + build.stderr
    build_ok = build.returncode == 0
    if not build_ok:
        return ChecksResult(False, 0, False, False, raw)

    test = _run(worktree, env, ["cargo", "test"])
    raw["test"] = test.stdout + test.stderr
    parsed = parse_test_failures(raw["test"])
    # cargo test упал, но счётчик не распарсился (напр. ошибка компиляции тестов) —
    # сигналим минимум одну неудачу, чтобы критерий не счёл прогон зелёным.
    test_failures = parsed if (parsed or test.returncode == 0) else 1

    clippy = _run(worktree, env, ["cargo", "clippy", "--all-targets", "--", "-D", "warnings"])
    raw["clippy"] = clippy.stdout + clippy.stderr
    clippy_ok = clippy.returncode == 0

    fmt = _run(worktree, env, ["cargo", "fmt", "--check"])
    raw["fmt"] = fmt.stdout + fmt.stderr
    fmt_ok = fmt.returncode == 0

    # Python-половина продукта (сам clave-dev). cargo её НЕ проверяет вовсе: правка в
    # tools/clave-dev могла бы «сойтись» на зелёном Rust, ничего не доказав. Гоняем своим
    # интерпретатором (в нём уже есть зависимости супервайзера).
    py_ok = True
    pkg = python_suite_dir(worktree)
    if pkg is not None:
        # Правила самопроверки зовём ПОИМЁННО, отдельно от discover. Иначе удалить правило можно
        # тихо: discover просто найдёт на один тест меньше и промолчит — проверено, 135 вместо 137
        # и зелёный результат. По имени удалённое правило даёт ModuleNotFoundError, то есть красный.
        rules = _run(
            worktree, env,
            [sys.executable, "-m", "unittest", *RULE_TESTS],
            cwd=pkg,
        )
        py = _run(worktree, env, [sys.executable, "-m", "unittest", "discover", "-s", "tests"], cwd=pkg)
        raw["python"] = rules.stdout + rules.stderr + py.stdout + py.stderr

        # Код возврата — ненадёжный сигнал: `sys.exit(0)` в импортируемом модуле гасит unittest
        # молча, и «ноль тестов» приходит с кодом 0. Поэтому требуем ПРЕДЪЯВИТЬ прогнанное.
        rules_ran = tests_ran(rules.stdout + rules.stderr)
        py_ran = tests_ran(py.stdout + py.stderr)
        py_ok = (
            rules.returncode == 0
            and py.returncode == 0
            # Хотя бы по одному тесту на каждое правило: иначе набор не запускался вовсе.
            and rules_ran >= len(RULE_TESTS)
            # Полный набор содержит правила, значит тестов в нём не меньше. Без магических чисел:
            # инвариант структурный, и порог не придётся двигать при каждом новом тесте.
            and py_ran >= rules_ran
        )

    return ChecksResult(True, test_failures, clippy_ok, fmt_ok, raw, py_ok)
