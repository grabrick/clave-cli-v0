import os
import stat
import tempfile
import unittest
from pathlib import Path

from clave_dev.agent import parse_clave_run, run_agent


class AgentStreamTest(unittest.TestCase):
    """Регресс на реальный баг: вывод агента забирался буферизованно → минуты тишины,
    а ответ агента (для аналитических задач он и есть результат) уходил в мусор."""

    def test_run_agent_streams_lines_live_and_hides_machine_marker(self):
        with tempfile.TemporaryDirectory() as d:
            fake = Path(d) / "clave"
            fake.write_text(
                "#!/bin/bash\n"
                "cat > /dev/null\n"
                'echo "агент читает код"\n'
                'echo "агент нашёл проблему"\n'
                "echo 'CLAVE-RUN {\"status\":\"completed\",\"code\":0,\"provider\":\"claude\"}'\n"
            )
            fake.chmod(fake.stat().st_mode | stat.S_IEXEC)

            seen = []
            result = run_agent(fake, Path(d), "задача", dict(os.environ), on_line=seen.append)

            self.assertEqual(result.status, "completed")
            self.assertIn("агент читает код", seen)
            self.assertIn("агент нашёл проблему", seen)
            # Машинную строку контракта наружу не показываем — её разбирают.
            self.assertTrue(all(not line.startswith("CLAVE-RUN") for line in seen))


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
