import unittest

from clave_dev.assertions import AssertionResult
from clave_dev.checks import ChecksResult
from clave_dev.context import build_context
from clave_dev.loop import converged, outcome


class OutcomeTest(unittest.TestCase):
    """Регресс на реальный баг: no-op агента объявлялся сходимостью."""

    def _green(self):
        return ChecksResult(build_ok=True, test_failures=0, clippy_ok=True, fmt_ok=True, raw={})

    def test_no_changes_is_not_convergence_even_when_checks_are_green(self):
        # Репозиторий был зелёным и ДО агента — зелёные проверки при пустом дифе
        # не доказывают ничего. Раньше это давало "converged: true" на любой задаче.
        self.assertEqual(outcome([], self._green(), []), "no_changes")

    def test_changes_plus_green_is_convergence(self):
        self.assertEqual(outcome(["src/x.rs"], self._green(), []), "converged")

    def test_changes_but_red_checks_continue(self):
        red = self._green()._replace(test_failures=2)
        self.assertEqual(outcome(["src/x.rs"], red, []), "continue")


class ConvergedTest(unittest.TestCase):
    def _checks(self, **kw):
        base = dict(build_ok=True, test_failures=0, clippy_ok=True, fmt_ok=True, raw={})
        base.update(kw)
        return ChecksResult(**base)

    def test_all_green_and_assertions_pass_converges(self):
        asserts = [AssertionResult("a", True, ""), AssertionResult("b", True, "")]
        self.assertTrue(converged(self._checks(), asserts))

    def test_failing_check_blocks(self):
        self.assertFalse(converged(self._checks(clippy_ok=False), []))
        self.assertFalse(converged(self._checks(test_failures=2), []))
        self.assertFalse(converged(self._checks(build_ok=False), []))
        self.assertFalse(converged(None, []))

    def test_failing_assertion_blocks(self):
        asserts = [AssertionResult("a", True, ""), AssertionResult("b", False, "nope")]
        self.assertFalse(converged(self._checks(), asserts))


class ContextTest(unittest.TestCase):
    def test_build_context_reports_checks_and_assertions(self):
        checks = ChecksResult(
            build_ok=True, test_failures=2, clippy_ok=False, fmt_ok=True,
            raw={"test": "line1\nfailure here\n", "clippy": "warning: x"},
        )
        asserts = [AssertionResult("visible('X')", False, "не найдено: 'X'")]
        text = build_context(checks, [["line a", "line b"]], asserts)
        self.assertIn("test failures: 2", text)
        self.assertIn("clippy: FAIL", text)
        self.assertIn("FAIL visible('X')", text)
        self.assertIn("line a", text)
