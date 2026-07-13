"""Суть провала проверки: ЧТО упало, а не сколько.

Вывод здесь настоящий — снят с живых провалов cargo, clippy и rustfmt на этом же репозитории.

Слепота была такая: человек в терминале видел `✗ test — 1 failed` и всё. Какой тест упал и почему,
уезжало ТОЛЬКО в промпт агента. Я сам на этом застрял посреди прогона и полез читать worktree
руками. Ровно то, что вчера чинили у зрения, — у проверок не заметили.
"""
import unittest

from clave_dev.failures import failure_lines, failure_payload

CARGO_TEST = """\
running 141 tests
test render::tests::narrow_footer_drops_git_and_keeps_the_slot ... FAILED
test ui::footer::tests::git_takes_room_before_hints_are_cut ... FAILED
test ui::footer::tests::layout_columns_are_pinned_to_exact_numbers ... ok

failures:

---- render::tests::narrow_footer_drops_git_and_keeps_the_slot stdout ----
thread 'render::tests::narrow_footer_drops_git_and_keeps_the_slot' panicked at src/render.rs:612:9:
assertion `left == right` failed
  left: 78
 right: 80

test result: FAILED. 139 passed; 2 failed; 0 ignored
"""

CLIPPY = """\
    Checking clave v0.1.3 (/Users/kirill/clave-cli)
error: function `unused_fn` is never used
 --> src/ui/helpers.rs:7:15
  |
7 | pub(crate) fn unused_fn() -> i32 { let x = 1; x }
  |               ^^^^^^^^^
  |
  = note: `-D dead-code` implied by `-D warnings`

error: returning the result of a `let` binding from a block
 --> src/ui/helpers.rs:7:47

error: could not compile `clave` (bin "clave") due to 2 previous errors
"""

FMT = """\
Diff in /Users/kirill/clave-cli/src/ui/helpers.rs:290:
 }
-fn    badly_formatted( )   ->i32{1}
+fn badly_formatted() -> i32 {
+    1
+}
"""

PY_UNITTEST = """\
FAIL: test_report_carries_unverified (tests.test_loop_wiring.FinalReportTest)
Traceback (most recent call last):
AssertionError: False is not true
ERROR: test_scenario_runs_binary_inside_the_worktree (tests.test_observer.ObserverTest)
TypeError: expected string or bytes-like object
Ran 166 tests in 15.1s
FAILED (failures=1, errors=1)
"""


class CargoTestTest(unittest.TestCase):
    def test_names_the_tests_that_failed(self):
        lines = failure_lines("test", CARGO_TEST)

        self.assertIn("render::tests::narrow_footer_drops_git_and_keeps_the_slot", lines)
        self.assertIn("ui::footer::tests::git_takes_room_before_hints_are_cut", lines)

    def test_a_passing_test_is_not_reported_as_failed(self):
        # Иначе человек побежит чинить зелёное.
        lines = failure_lines("test", CARGO_TEST)
        self.assertFalse(
            any("layout_columns_are_pinned_to_exact_numbers" in line for line in lines)
        )

    def test_the_panic_says_where_and_why(self):
        lines = failure_lines("test", CARGO_TEST)

        self.assertTrue(
            any("src/render.rs:612:9" in line and "assertion" in line for line in lines),
            lines,
        )


class ClippyAndFmtTest(unittest.TestCase):
    def test_clippy_errors_are_extracted(self):
        lines = failure_lines("clippy", CLIPPY)

        self.assertIn("error: function `unused_fn` is never used", lines)
        self.assertIn("error: returning the result of a `let` binding from a block", lines)

    def test_fmt_names_the_file(self):
        lines = failure_lines("fmt", FMT)

        self.assertEqual(len(lines), 1)
        self.assertIn("src/ui/helpers.rs", lines[0])


class PythonSuiteTest(unittest.TestCase):
    def test_both_failures_and_errors_are_named(self):
        lines = failure_lines("python", PY_UNITTEST)

        self.assertTrue(any("test_report_carries_unverified" in line for line in lines))
        self.assertTrue(any("test_scenario_runs_binary_inside_the_worktree" in line for line in lines))


class TruncationTest(unittest.TestCase):
    def test_a_green_check_has_nothing_to_explain(self):
        # Объяснение провала там, где провала нет, — шум, и его перестанут читать.
        self.assertEqual(failure_lines("test", "test result: ok. 141 passed; 0 failed"), [])

    def test_truncation_is_announced_not_silent(self):
        # Сотня упавших тестов в терминале нечитаема. Но молчаливое урезание — это враньё:
        # человек решит, что упало ровно столько, сколько показали.
        many = "\n".join(f"test t::{i} ... FAILED" for i in range(20))

        payload = failure_payload("test", many, limit=6)

        self.assertEqual(len(payload["failures"]), 6)
        self.assertEqual(payload["failures_truncated"], 14)

    def test_nothing_to_truncate_says_nothing(self):
        payload = failure_payload("test", CARGO_TEST, limit=6)
        self.assertNotIn("failures_truncated", payload)


if __name__ == "__main__":
    unittest.main()
