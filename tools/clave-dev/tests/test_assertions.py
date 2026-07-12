import unittest

from clave_dev.assertions import (
    clean_exit,
    evaluate,
    line_matches,
    no_line_overflows_width,
    not_visible,
    visible,
)


class AssertionsTest(unittest.TestCase):
    def setUp(self):
        self.grid = ["  Отправка", "  Enter  отправить", "  Ctrl+R поиск"]

    def test_visible_and_not_visible(self):
        self.assertTrue(visible("Отправка")(self.grid, 0).passed)
        self.assertFalse(visible("Управление")(self.grid, 0).passed)
        self.assertTrue(not_visible("Управление")(self.grid, 0).passed)
        self.assertFalse(not_visible("Отправка")(self.grid, 0).passed)

    def test_line_matches_and_overflow_and_exit(self):
        self.assertTrue(line_matches(r"Enter\s+отправить")(self.grid, 0).passed)
        self.assertTrue(no_line_overflows_width(40)(self.grid, 0).passed)
        self.assertFalse(no_line_overflows_width(5)(self.grid, 0).passed)
        self.assertFalse(clean_exit()(self.grid, 1).passed)
        self.assertTrue(clean_exit()(self.grid, 0).passed)

    def test_evaluate_returns_result_per_assertion(self):
        results = evaluate([visible("Отправка"), not_visible("X")], self.grid, 0)
        self.assertEqual(len(results), 2)
        self.assertTrue(all(r.passed for r in results))
