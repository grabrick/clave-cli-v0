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

    def test_no_result_line_is_zero(self):
        self.assertEqual(parse_test_failures("compiler error, no tests ran"), 0)
