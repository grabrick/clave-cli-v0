import subprocess
import unittest
from pathlib import Path
from unittest import mock

from clave_dev import checks
from clave_dev.checks import RULE_TESTS, parse_test_failures, run_checks, tests_ran


class ChecksParseTest(unittest.TestCase):
    def test_zero_failures(self):
        out = "test result: ok. 107 passed; 0 failed; 0 ignored; 0 measured"
        self.assertEqual(parse_test_failures(out), 0)

    def test_counts_failures_across_result_lines(self):
        out = (
            "test result: FAILED. 5 passed; 2 failed; 0 ignored\n"
            "test result: FAILED. 3 passed; 1 failed; 0 ignored\n"
        )
        self.assertEqual(parse_test_failures(out), 3)

    def test_counts_failures_from_cargo_output_with_failure_noise(self):
        out = """
running 3 tests
test app::tests::passes ... ok
test app::tests::fails ... FAILED

failures:

---- app::tests::fails stdout ----
thread 'app::tests::fails' panicked at src/app.rs:42:9:
assertion `left == right` failed

failures:
    app::tests::fails

test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

error: test failed, to rerun pass `--lib`
"""
        self.assertEqual(parse_test_failures(out), 1)

    def test_no_result_line_is_zero(self):
        self.assertEqual(parse_test_failures("compiler error, no tests ran"), 0)


class TestsRanTest(unittest.TestCase):
    """Прогон, который НЕ СОСТОЯЛСЯ, не должен читаться как прогон, который прошёл."""

    def test_counts_what_the_suite_actually_ran(self):
        self.assertEqual(tests_ran("Ran 178 tests in 5.470s\n\nOK\n"), 178)
        self.assertEqual(tests_ran("Ran 1 test in 0.01s"), 1)

    def test_silence_is_zero_not_success(self):
        # Тот самый случай: `sys.exit(0)` в импортируемом модуле гасит unittest — пустой вывод,
        # ноль тестов, КОД 0. Это провал, а не «нечего проверять».
        self.assertEqual(tests_ran(""), 0)
        self.assertEqual(tests_ran("Traceback (most recent call last):\n  ...\n"), 0)


class PyOkTrustsCountsNotExitCodesTest(unittest.TestCase):
    """Тест на МЕСТО ВЫЗОВА, а не на функцию.

    Юнит-тест `tests_ran` останется зелёным, даже если из `run_checks` выкинуть её вызов — и гейт
    снова начнёт верить коду возврата. Ровно эта ошибка (проверил функцию, не проверил вызов) уже
    стоила мне падения `/dev` на TypeError в `_emit_final`. Поэтому проверяем сам `run_checks`.
    """

    def _run_checks_with(self, rules_out: str, py_out: str, worktree: Path):
        ok = subprocess.CompletedProcess([], 0, "", "")

        def fake_run(_wt, _env, args, cwd=None):
            if "unittest" not in args:
                return ok  # cargo build/test/clippy/fmt — зелёные
            return subprocess.CompletedProcess(
                args, 0, "", py_out if "discover" in args else rules_out
            )

        with mock.patch.object(checks, "_run", side_effect=fake_run):
            return run_checks(worktree, {}, "debug")

    def setUp(self):
        import tempfile

        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.wt = Path(self.tmp.name)
        (self.wt / "tools" / "clave-dev" / "clave_dev").mkdir(parents=True)

    def test_zero_tests_with_exit_code_zero_is_not_green(self):
        # Набор правил не запустился вовсе — а код 0. Раньше это читалось как «правила прошли».
        res = self._run_checks_with("", "Ran 178 tests in 5s\n\nOK\n", self.wt)
        self.assertFalse(res.py_ok, "ноль прогнанных правил с кодом 0 прочитан как успех")

    def test_a_gutted_discover_is_not_green_either(self):
        # Правила прогнались, а полный набор — нет. Он ВКЛЮЧАЕТ правила, значит меньше их быть
        # не может: расхождение означает, что discover не состоялся.
        res = self._run_checks_with("Ran 20 tests in 2s\n\nOK\n", "Ran 0 tests in 0.0s\n\nOK\n", self.wt)
        self.assertFalse(res.py_ok)

    def test_a_real_green_run_is_still_green(self):
        # Гейт, который ругается всегда, — тоже декорация: его отключат первым.
        res = self._run_checks_with(
            f"Ran {len(RULE_TESTS) + 16} tests in 2s\n\nOK\n", "Ran 178 tests in 5s\n\nOK\n", self.wt
        )
        self.assertTrue(res.py_ok)


class PythonSuiteTest(unittest.TestCase):
    def test_detects_clave_dev_package_in_worktree(self):
        import tempfile

        from clave_dev.checks import python_suite_dir

        with tempfile.TemporaryDirectory() as d:
            wt = Path(d)
            self.assertIsNone(python_suite_dir(wt))  # пакета нет → проверку не гоняем
            (wt / "tools" / "clave-dev" / "clave_dev").mkdir(parents=True)
            self.assertEqual(python_suite_dir(wt), wt / "tools" / "clave-dev")
