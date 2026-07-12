import unittest

from clave_dev.vision import FakeVisionProvider
from clave_dev.vision_probe import probe_summary


def _verdict(d):
    return FakeVisionProvider(d).analyze_image(None)


class ProbeSummaryTest(unittest.TestCase):
    def test_pass_verdict_exit_0(self):
        v = _verdict({"checklist_results": [{"check": "c", "required": True, "passed": True}]})
        summary, code = probe_summary(v)
        self.assertTrue(summary["pass"])
        self.assertEqual(code, 0)

    def test_failed_required_exit_1_and_listed(self):
        v = _verdict({"checklist_results": [{"check": "правая граница", "required": True, "passed": False}]})
        summary, code = probe_summary(v)
        self.assertFalse(summary["pass"])
        self.assertEqual(code, 1)
        self.assertIn("правая граница", summary["failed_required"])

    def test_high_open_issue_blocks(self):
        v = _verdict({"issues": [{"description": "срез справа", "severity": "high", "source": "open"}]})
        summary, code = probe_summary(v)
        self.assertFalse(summary["pass"])
        self.assertEqual(summary["issues"][0]["severity"], "high")
