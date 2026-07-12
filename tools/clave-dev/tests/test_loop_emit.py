import json
import unittest

from clave_dev.emit import Emitter, format_line
from clave_dev.loop import _vision_payload
from clave_dev.visual_verdict import ChecklistItem, Issue, VisionVerdict


class CapturingOut:
    def __init__(self):
        self.lines = []

    def write(self, s):
        if s.strip():
            self.lines.append(s.strip())

    def flush(self):
        pass


class LoopEmitTest(unittest.TestCase):
    def test_emitter_lines_are_framed_by_type(self):
        out = CapturingOut()
        em = Emitter(enabled=True, out=out)
        em.progress("round 1")
        em.check({"name": "build", "ok": True})
        em.report({"converged": False, "rounds": 1})
        self.assertTrue(any(l.startswith("CLAVE-DEV progress") for l in out.lines))
        self.assertTrue(any(l.startswith("CLAVE-DEV check") for l in out.lines))
        self.assertTrue(any(l.startswith("CLAVE-DEV report") for l in out.lines))


def _verdict(checklist=(), issues=()):
    return VisionVerdict(list(issues), list(checklist), "", "{}")


class VisionPayloadTest(unittest.TestCase):
    """Payload зрения обязан нести причину блокировки, а не только счётчики."""

    def test_payload_carries_failed_required_and_findings(self):
        v = _verdict(
            checklist=[
                ChecklistItem("нет обрезанных глифов", True, False, "правая рамка срезана"),
                ChecklistItem("логотип по центру", False, False, "смещён на 1 колонку"),
                ChecklistItem("рамка сплошная", True, True, ""),
            ],
            issues=[
                Issue("полый курсор", "high", "footer", "open"),
                Issue("логотип тусклый", "low", None, "open"),
            ],
        )
        payload = _vision_payload([v], ("high", "medium"))

        self.assertEqual(payload["pass"], False)
        self.assertEqual(payload["issues"], 2)
        self.assertEqual(payload["regressions"], 1)
        self.assertEqual(
            payload["failed_required"],
            ["сценарий 0: нет обрезанных глифов — правая рамка срезана"],
        )
        # Упавший optional-пункт не блокирует прогон, и в перечень причин ему нельзя.
        self.assertNotIn("логотип по центру", " ".join(payload["failed_required"]))
        self.assertEqual(
            payload["findings"],
            ["сценарий 0: [high] полый курсор (region=footer)", "сценарий 0: [low] логотип тусклый"],
        )

    def test_multiline_judge_prose_collapses_to_one_short_line(self):
        long_text = "первая строка\nвторая строка   с пробелами\n" + "хвост " * 60
        v = _verdict(issues=[Issue(long_text, "medium", None, "open")])
        payload = _vision_payload([v], ("high", "medium"))

        finding = payload["findings"][0]
        self.assertNotIn("\n", finding)
        self.assertIn("первая строка вторая строка с пробелами", finding)
        self.assertTrue(finding.endswith("…"))
        self.assertLessEqual(len(finding), len("сценарий 0: [medium] ") + 120)
        # И обрамлённая строка остаётся одной строкой валидного JSON.
        line = format_line("vision", payload)
        self.assertEqual(len(line.splitlines()), 1)
        json.loads(line[len("CLAVE-DEV vision "):])
