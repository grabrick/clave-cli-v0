import unittest

from clave_dev.agent import parse_clave_run


class AgentParseTest(unittest.TestCase):
    def test_parses_completed_line(self):
        out = (
            "activity...\n"
            'CLAVE-RUN {"status":"completed","code":0,"provider":"codex",'
            '"usage":{"input":60,"output":30},"ended_reason":"completed"}\n'
        )
        r = parse_clave_run(out, 0)
        self.assertEqual(r.status, "completed")
        self.assertEqual(r.code, 0)
        self.assertEqual(r.provider, "codex")
        self.assertEqual(r.usage["input"], 60)
        self.assertEqual(r.exit_code, 0)

    def test_ignores_earlier_lines_and_takes_last_marker(self):
        out = (
            'CLAVE-RUN {"status":"cancelled","provider":"claude"}\n'
            'CLAVE-RUN {"status":"completed","code":0,"provider":"claude"}\n'
        )
        r = parse_clave_run(out, 0)
        self.assertEqual(r.status, "completed")

    def test_no_marker(self):
        r = parse_clave_run("just some output", 3)
        self.assertEqual(r.status, "no_marker")
        self.assertEqual(r.exit_code, 3)
