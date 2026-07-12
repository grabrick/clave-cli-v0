import unittest
from pathlib import Path

from clave_dev.assertions import AssertionResult
from clave_dev.checks import ChecksResult
from clave_dev.context import build_visual_context
from clave_dev.loop import converged
from clave_dev.vision import FakeVisionProvider
from clave_dev.visual_observer import run_visual
from clave_dev.visual_verdict import verdict_passes


def _green_checks():
    return ChecksResult(build_ok=True, test_failures=0, clippy_ok=True, fmt_ok=True, raw={})


def _verdict(d):
    return FakeVisionProvider(d).analyze_image(None)


class VisualConvergeTest(unittest.TestCase):
    def test_vision_fail_blocks_even_if_text_green(self):
        bad = _verdict({"checklist_results": [{"check": "правая граница", "required": True, "passed": False}]})
        self.assertFalse(converged(_green_checks(), [AssertionResult("a", True, "")], [bad]))

    def test_all_green_with_passing_vision_converges(self):
        good = _verdict({"checklist_results": [{"check": "ok", "required": True, "passed": True}]})
        self.assertTrue(converged(_green_checks(), [AssertionResult("a", True, "")], [good]))

    def test_no_vision_verdicts_keeps_phase1_behavior(self):
        self.assertTrue(converged(_green_checks(), [AssertionResult("a", True, "")]))

    def test_build_visual_context_lists_failed_checks_and_critique(self):
        v = _verdict({"checklist_results": [{"check": "правая граница", "required": True, "passed": False, "note": "срез"}],
                      "open_critique": "иначе ок"})
        text = build_visual_context([v])
        self.assertIn("правая граница", text)
        self.assertIn("иначе ок", text)


class RunVisualFailSafeTest(unittest.TestCase):
    """§8: любая беда захвата/зрения → блокирующий вердикт, никогда тихий pass."""

    def test_screencapture_error_blocks(self):
        v = run_visual(42, FakeVisionProvider({}), lambda cmd: 1, lambda p: b"\xff" * 100, Path("/x.png"))
        self.assertFalse(verdict_passes(v))

    def test_blank_frame_blocks(self):
        v = run_visual(42, FakeVisionProvider({}), lambda cmd: 0, lambda p: bytes(100), Path("/x.png"))
        self.assertFalse(verdict_passes(v))

    def test_good_frame_uses_vision_verdict(self):
        good = FakeVisionProvider({"checklist_results": [{"check": "ok", "required": True, "passed": True}]})
        v = run_visual(42, good, lambda cmd: 0, lambda p: b"\xff" * 100, Path("/x.png"))
        self.assertTrue(verdict_passes(v))

    def test_vision_exception_blocks(self):
        unavailable = FakeVisionProvider({}, available=False)  # analyze_image бросает
        v = run_visual(42, unavailable, lambda cmd: 0, lambda p: b"\xff" * 100, Path("/x.png"))
        self.assertFalse(verdict_passes(v))
