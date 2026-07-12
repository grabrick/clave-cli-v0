import io
import unittest

from clave_dev.emit import Emitter, format_line


class EmitTest(unittest.TestCase):
    def test_format_text_and_json_types(self):
        self.assertEqual(format_line("progress", "раунд 1"), "CLAVE-DEV progress раунд 1")
        line = format_line("check", {"name": "build", "ok": True})
        self.assertTrue(line.startswith("CLAVE-DEV check "))
        self.assertIn('"name": "build"', line)

    def test_unknown_type_raises(self):
        with self.assertRaises(ValueError):
            format_line("nope", "x")

    def test_disabled_emitter_is_silent(self):
        buf = io.StringIO()
        Emitter(enabled=False, out=buf).progress("тихо")
        self.assertEqual(buf.getvalue(), "")

    def test_enabled_emitter_writes_framed_line(self):
        buf = io.StringIO()
        Emitter(enabled=True, out=buf).report({"converged": True, "rounds": 1})
        self.assertIn("CLAVE-DEV report ", buf.getvalue())
        self.assertIn('"converged": true', buf.getvalue())
