import unittest
from pathlib import Path

from clave_dev.capture import is_blank_frame, screencapture_cmd
from clave_dev.terminal_driver import launch_applescript, send_line_applescript


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

    def test_launch_isolates_home_returns_window_id_and_does_not_steal_focus(self):
        script = launch_applescript(
            Path("/wt/target/debug/clave"),
            "clave-dev xyz",
            Path("/wt"),
            "CLAVE_HOME=/tmp/h CLAVE_SKIP_ONBOARDING=1 ",
        )
        self.assertIn("clave-dev xyz", script)
        self.assertIn("do script", script)
        # Без изоляции наблюдаемый clave лез бы в РЕАЛЬНЫЙ конфиг и чаты пользователя.
        self.assertIn("CLAVE_HOME=/tmp/h", script)
        # id окна нужен, чтобы писать адресно в его tty, а не «фронтовому».
        self.assertIn("return id of w", script)
        # Прогон фоновый — фокус у пользователя не воруем.
        self.assertNotIn("activate", script)

    def test_send_line_targets_the_window_and_never_injects_globally(self):
        script = send_line_applescript(42, 'say "hi"')
        self.assertIn("window id 42", script)
        self.assertIn('\\"', script)
        # System Events keystroke — ГЛОБАЛЬНАЯ инъекция: в фоновом прогоне могла бы
        # прилететь в чужое приложение. Этого пути больше нет.
        self.assertNotIn("System Events", script)
