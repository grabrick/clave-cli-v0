import unittest
from pathlib import Path

from clave_dev.vision import VisionUnavailableError
from clave_dev.vision_claude import ClaudeVisionProvider, select_vision
from clave_dev.visual_verdict import verdict_passes


class ClaudeVisionTest(unittest.TestCase):
    def test_unavailable_without_key_or_sender(self):
        p = ClaudeVisionProvider(env={})
        self.assertFalse(p.available())
        with self.assertRaises(VisionUnavailableError):
            p.analyze_image(Path("/x.png"))

    def test_sender_channel_parses_verdict(self):
        raw = '```json\n{"checklist_results":[{"check":"c","required":true,"passed":true}]}\n```'
        p = ClaudeVisionProvider(env={}, sender=lambda png, prompt: raw)
        self.assertTrue(p.available())
        self.assertTrue(verdict_passes(p.analyze_image(Path("/x.png"))))

    def test_select_vision_none_disables(self):
        self.assertIsNone(select_vision(None))
        self.assertIsInstance(
            select_vision("claude", env={"ANTHROPIC_API_KEY": "k"}), ClaudeVisionProvider
        )
