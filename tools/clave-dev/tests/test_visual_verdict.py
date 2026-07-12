import unittest

from clave_dev.visual_verdict import (
    VerdictParseError,
    extract_verdict_json,
    parse_verdict,
    verdict_passes,
)


class ParseTest(unittest.TestCase):
    def test_parse_defaults_are_fail_safe(self):
        v = parse_verdict({"issues": [{"description": "x", "severity": "weird"}],
                           "checklist_results": [{"check": "c"}]})
        self.assertEqual(v.issues[0].severity, "high")   # неизвестный severity → high
        self.assertTrue(v.checklist[0].required)          # required по умолчанию True
        self.assertFalse(v.checklist[0].passed)           # passed по умолчанию False

    def test_extract_json_from_wrapped_text(self):
        text = 'вот вердикт:\n```json\n{"open_critique": "ок"}\n```\nконец'
        self.assertEqual(extract_verdict_json(text)["open_critique"], "ок")

    def test_extract_json_raises_on_garbage(self):
        with self.assertRaises(VerdictParseError):
            extract_verdict_json("никакого json тут нет")


class PassTest(unittest.TestCase):
    def test_all_good_passes(self):
        v = parse_verdict({"checklist_results": [{"check": "c", "required": True, "passed": True}]})
        self.assertTrue(verdict_passes(v))

    def test_required_checklist_failure_blocks_even_with_low_or_no_issue(self):
        v = parse_verdict({"checklist_results": [{"check": "c", "required": True, "passed": False}],
                           "issues": [{"description": "мелочь", "severity": "low"}]})
        self.assertFalse(verdict_passes(v))   # required-провал блокирует вопреки low

    def test_optional_high_issue_blocks(self):
        v = parse_verdict({"checklist_results": [{"check": "c", "required": False, "passed": True}],
                           "issues": [{"description": "big", "severity": "high", "source": "open"}]})
        self.assertFalse(verdict_passes(v))

    def test_optional_low_issue_passes(self):
        v = parse_verdict({"issues": [{"description": "tiny", "severity": "low", "source": "open"}]})
        self.assertTrue(verdict_passes(v))


class ThresholdTest(unittest.TestCase):
    def test_severities_at_or_above(self):
        from clave_dev.visual_verdict import severities_at_or_above

        self.assertEqual(severities_at_or_above("low"), ("low", "medium", "high"))
        self.assertEqual(severities_at_or_above("medium"), ("medium", "high"))
        self.assertEqual(severities_at_or_above("high"), ("high",))

    def test_none_threshold_gates_on_required_checklist_only(self):
        # Режим автономной петли: мнения модели НЕ блокируют (иначе агент погонится за
        # эстетическими призраками вроде «логотип тусклый»), а объективный required —
        # блокирует. Именно required и поймал реальный баг среза футера.
        from clave_dev.visual_verdict import severities_at_or_above

        blocking = severities_at_or_above("none")
        self.assertEqual(blocking, ())

        opinionated = parse_verdict({
            "checklist_results": [{"check": "правая граница", "required": True, "passed": True}],
            "issues": [
                {"description": "логотип тусклый", "severity": "medium", "source": "open"},
                {"description": "разделитель короче на колонку", "severity": "high", "source": "open"},
            ],
        })
        self.assertTrue(verdict_passes(opinionated, blocking), "мнения модели не блокируют")

        real_defect = parse_verdict({
            "checklist_results": [{"check": "правая граница", "required": True, "passed": False}],
        })
        self.assertFalse(verdict_passes(real_defect, blocking), "required-провал блокирует всегда")
