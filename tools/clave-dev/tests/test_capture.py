import unittest
from pathlib import Path

from clave_dev.capture import is_blank_frame, screencapture_cmd
from clave_dev.terminal_driver import keystroke_applescript, launch_applescript


class CaptureTest(unittest.TestCase):
    def test_screencapture_cmd_targets_window(self):
        cmd = screencapture_cmd(42, Path("/tmp/o.png"))
        self.assertEqual(cmd[:1], ["screencapture"])
        self.assertIn("-l42", cmd)
        self.assertIn("/tmp/o.png", cmd)

    def test_is_blank_frame_detects_black_and_content(self):
        self.assertTrue(is_blank_frame(bytes(1000)))              # всё нули → пусто
        self.assertTrue(is_blank_frame(b""))                       # пусто → пусто
        self.assertFalse(is_blank_frame(bytes([200]) * 1000))      # насыщено → не пусто

    def test_launch_applescript_sets_unique_title(self):
        script = launch_applescript(Path("/wt/target/debug/clave"), "clave-dev xyz", Path("/wt"))
        self.assertIn("clave-dev xyz", script)
        self.assertIn("do script", script)

    def test_keystroke_applescript_escapes_quotes(self):
        self.assertIn('\\"', keystroke_applescript('say "hi"'))
