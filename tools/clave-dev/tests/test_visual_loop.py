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
    """§8: любая беда захвата/зрения → блокирующий вердикт, никогда тихий pass.

    run_visual отдаёт СПИСОК выборок: один кадр судится несколько раз, потому что судья
    невоспроизводим (замерено: три разных вердикта из пяти на неизменном продукте)."""

    def test_screencapture_error_blocks(self):
        vs = run_visual(42, FakeVisionProvider({}), lambda cmd: 1, lambda p: b"\xff" * 100, Path("/x.png"))
        self.assertFalse(verdict_passes(vs[0]))

    def test_blank_frame_blocks(self):
        vs = run_visual(42, FakeVisionProvider({}), lambda cmd: 0, lambda p: bytes(100), Path("/x.png"))
        self.assertFalse(verdict_passes(vs[0]))

    def test_good_frame_uses_vision_verdict(self):
        good = FakeVisionProvider({"checklist_results": [{"check": "ok", "required": True, "passed": True}]})
        vs = run_visual(42, good, lambda cmd: 0, lambda p: b"\xff" * 100, Path("/x.png"))
        self.assertTrue(verdict_passes(vs[0]))

    def test_vision_exception_blocks(self):
        unavailable = FakeVisionProvider({}, available=False)  # analyze_image бросает
        vs = run_visual(42, unavailable, lambda cmd: 0, lambda p: b"\xff" * 100, Path("/x.png"))
        self.assertFalse(verdict_passes(vs[0]))

    def test_one_capture_is_judged_as_many_times_as_asked(self):
        # Кадр снимаем ОДИН раз, судим N: разброс даёт судья, а не скриншот. Пересъёмка окна
        # ради выборок была бы дороже и мешала бы шум судьи с дрожанием курсора.
        captures = []
        good = FakeVisionProvider({"checklist_results": [{"check": "ok", "required": True, "passed": True}]})
        vs = run_visual(
            42, good, lambda cmd: captures.append(cmd) or 0, lambda p: b"\xff" * 100,
            Path("/x.png"), samples=3,
        )
        self.assertEqual(len(vs), 3)
        self.assertEqual(len(captures), 1)


class VisionThatNeverRanTest(unittest.TestCase):
    """Гейт обязан УМЕТЬ провалиться. Этот — не умел.

    `all([])` — это `True`, поэтому пустой список вердиктов читался как «зрение не возражает».
    Но пустым он бывает и когда зрение просто НЕ ОТРАБОТАЛО: бэкенд отвалился посреди прогона
    (preflight ловит только старт). Человек просил проверку глазами, её не было — а ему
    рапортовали «сошлось».
    """

    def test_requested_vision_that_produced_nothing_cannot_converge(self):
        self.assertFalse(
            converged(_green_checks(), [AssertionResult("a", True, "")], [], vision_required=True)
        )

    def test_text_only_run_still_converges_without_vision(self):
        # Фаза 1: зрения не просили — пустой список законен.
        self.assertTrue(
            converged(_green_checks(), [AssertionResult("a", True, "")], [], vision_required=False)
        )

    def test_outcome_reports_continue_not_converged(self):
        from clave_dev.loop import outcome

        got = outcome(
            ["src/x.rs"], _green_checks(), [AssertionResult("a", True, "")], [],
            vision_required=True,
        )
        self.assertEqual(got, "continue")

    def test_config_does_not_arm_the_opinion_gate_by_default(self):
        # RunConfig без явного порога не должен возвращать абсолютный гейт по мнениям.
        from clave_dev.loop import RunConfig

        cfg = RunConfig(
            known_good=None, worktree=None, repo=None, env={}, profile="debug",
            task="t", effort=None, rounds=None, max_rounds=1, scenarios=[],
        )
        self.assertEqual(cfg.blocking_severities, ())
