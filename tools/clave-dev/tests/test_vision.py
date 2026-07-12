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
