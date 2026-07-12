import unittest
from pathlib import Path

from clave_dev.vision import FakeVisionProvider, VisionUnavailableError
from clave_dev.visual_verdict import verdict_passes


class VisionInterfaceTest(unittest.TestCase):
    def test_fake_returns_parsed_verdict(self):
        fake = FakeVisionProvider({"checklist_results": [{"check": "c", "required": True, "passed": True}]})
        self.assertTrue(fake.available())
        v = fake.analyze_image(Path("/nope.png"))
        self.assertTrue(verdict_passes(v))

    def test_unavailable_raises(self):
        fake = FakeVisionProvider({}, available=False)
        self.assertFalse(fake.available())
        with self.assertRaises(VisionUnavailableError):
            fake.analyze_image(Path("/nope.png"))


class VisionPreflightTest(unittest.TestCase):
    """Проверяем возможность зрения ДО старта: иначе узнаем на 3-м раунде, а fail-safe
    вердикты превратят каждый раунд в гарантированную не-сходимость."""

    def test_no_backend(self):
        from clave_dev.vision import vision_preflight

        reason = vision_preflight(FakeVisionProvider({}, available=False))
        self.assertIn("бэкенд", reason)

    def test_no_quartz(self):
        from clave_dev.vision import vision_preflight

        reason = vision_preflight(
            FakeVisionProvider({}), capture=lambda: None, quartz=lambda: False
        )
        self.assertIn("Quartz", reason)

    def test_screen_capture_unavailable(self):
        from clave_dev.vision import vision_preflight

        reason = vision_preflight(
            FakeVisionProvider({}),
            capture=lambda: "screencapture не смог снять экран",
            quartz=lambda: True,
        )
        self.assertIn("screencapture", reason)

    def test_all_good(self):
        from clave_dev.vision import vision_preflight

        self.assertIsNone(
            vision_preflight(FakeVisionProvider({}), capture=lambda: None, quartz=lambda: True)
        )
