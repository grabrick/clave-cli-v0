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


class VisionApiTest(unittest.TestCase):
    def test_build_request_has_image_and_text_blocks(self):
        import json

        from clave_dev.vision_claude import ANTHROPIC_URL, build_vision_request

        url, headers, body = build_vision_request("QUJD", "оцени", "claude-sonnet-5", "sk-xxx")
        self.assertEqual(url, ANTHROPIC_URL)
        self.assertEqual(headers["x-api-key"], "sk-xxx")
        self.assertIn("anthropic-version", headers)
        payload = json.loads(body)
        self.assertEqual(payload["model"], "claude-sonnet-5")
        types = [b["type"] for b in payload["messages"][0]["content"]]
        self.assertIn("image", types)
        self.assertIn("text", types)

    def test_extract_text_joins_and_handles_empty(self):
        from clave_dev.vision_claude import extract_vision_text

        resp = {"content": [{"type": "text", "text": "часть1 "}, {"type": "text", "text": "часть2"}]}
        self.assertEqual(extract_vision_text(resp), "часть1 часть2")
        self.assertEqual(extract_vision_text({}), "")


class ClaudeCliVisionTest(unittest.TestCase):
    """Канал зрения через авторизованный claude CLI — без ANTHROPIC_API_KEY (спека §3)."""

    def test_cli_prompt_carries_path_and_json_demand(self):
        from clave_dev.vision_claude import build_cli_vision_prompt

        p = build_cli_vision_prompt("/tmp/frame.png", "чеклист")
        self.assertIn("/tmp/frame.png", p)
        self.assertIn("JSON", p)

    def test_cli_provider_parses_verdict_from_runner(self):
        from clave_dev.vision_claude import ClaudeCliVisionProvider

        raw = 'вот результат:\n{"checklist_results":[{"check":"c","required":true,"passed":true}]}'
        p = ClaudeCliVisionProvider(runner=lambda prompt: raw)
        self.assertTrue(p.available())
        self.assertTrue(verdict_passes(p.analyze_image(Path("/x.png"))))

    def test_select_vision_claude_cli(self):
        from clave_dev.vision_claude import ClaudeCliVisionProvider

        self.assertIsInstance(select_vision("claude-cli"), ClaudeCliVisionProvider)
