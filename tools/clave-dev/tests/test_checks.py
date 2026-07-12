import unittest

from clave_dev.checks import parse_test_failures


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


class PythonSuiteTest(unittest.TestCase):
    def test_detects_clave_dev_package_in_worktree(self):
        import tempfile
        from pathlib import Path

        from clave_dev.checks import python_suite_dir

        with tempfile.TemporaryDirectory() as d:
            wt = Path(d)
            self.assertIsNone(python_suite_dir(wt))  # пакета нет → проверку не гоняем
            (wt / "tools" / "clave-dev" / "clave_dev").mkdir(parents=True)
            self.assertEqual(python_suite_dir(wt), wt / "tools" / "clave-dev")
