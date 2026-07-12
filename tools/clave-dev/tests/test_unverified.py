"""ПРАВИЛО 2: «сошлось» не имеет права стоять в одиночку."""
import unittest

from clave_dev.unverified import tests_added, unverified

RUST_WITH_TEST = """\
--- a/src/ui/footer.rs
+++ b/src/ui/footer.rs
+pub(crate) fn pick(x: usize) -> usize { x }
+
+    #[test]
+    fn pinned_segment_is_the_widest() {
+        assert_eq!(pick(1), 1);
+    }
"""

RUST_NO_TEST = """\
--- a/src/app/footer.rs
+++ b/src/app/footer.rs
+const GIT_REF_TTL: Duration = Duration::from_secs(2);
+        self.git_ref = detect_git_ref(&self.resolved_work_dir());
"""

PY_WITH_TEST = """\
--- a/tools/clave-dev/tests/test_x.py
+++ b/tools/clave-dev/tests/test_x.py
+def test_it_blocks_on_a_bad_input():
+    assert not gate(bad)
"""


class TestsAddedTest(unittest.TestCase):
    def test_counts_rust_tests_which_live_in_the_same_file(self):
        # Считать по путям нельзя: в Rust тест лежит рядом с кодом, и правка с тестами
        # выглядела бы как правка без них.
        self.assertEqual(tests_added(RUST_WITH_TEST), 1)

    def test_counts_python_tests(self):
        self.assertEqual(tests_added(PY_WITH_TEST), 1)

    def test_a_change_without_tests_counts_zero(self):
        self.assertEqual(tests_added(RUST_NO_TEST), 0)


class UnverifiedTest(unittest.TestCase):
    def test_it_always_says_correctness_was_not_checked(self):
        # Даже на самом зелёном исходе. «Сошлось» означает «не сломал», а не «сделал».
        lines = unverified(["src/x.rs"], RUST_WITH_TEST, converged=True)

        self.assertTrue(any("не проверяла и не умеет" in line for line in lines))
        self.assertTrue(any("Читай дифф" in line for line in lines))

    def test_a_change_with_no_tests_is_called_out(self):
        # Ровно так в этот проект попал спавн git каждые две секунды: гейты зелёные, тестов нет,
        # не заметил никто — ни исполнитель, ни критик.
        lines = unverified(["src/app/footer.rs"], RUST_NO_TEST, converged=True)

        self.assertTrue(
            any("добавлено тестов: 0" in line and "компилируется" in line for line in lines),
            f"правку без тестов надо называть вслух: {lines}",
        )

    def test_a_change_with_tests_is_not_scolded(self):
        lines = unverified(["src/ui/footer.rs"], RUST_WITH_TEST, converged=True)

        self.assertFalse(any("добавлено тестов: 0" in line for line in lines))
        self.assertTrue(any("добавлено тестов: 1" in line for line in lines))

    def test_a_run_that_did_not_converge_says_so_too(self):
        lines = unverified(["src/x.rs"], RUST_WITH_TEST, converged=False)

        self.assertTrue(any("не «сошлось»" in line for line in lines))


if __name__ == "__main__":
    unittest.main()
