import contextlib
import io
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from clave_dev.diff import build_diff
from clave_dev.emit import Emitter, format_line, human_lines, no_op_emitter

VISION_WITH_DETAILS = {
    "pass": False,
    "issues": 2,
    "regressions": 1,
    "failed_required": ["сценарий 0: нет обрезанных глифов — правая рамка срезана"],
    "findings": [
        "сценарий 0: [high] полый курсор (region=footer)",
        "сценарий 0: [low] логотип тусклый",
    ],
}


class EmitTest(unittest.TestCase):
    def test_format_text_and_json_types(self):
        self.assertEqual(format_line("progress", "раунд 1"), "CLAVE-DEV progress раунд 1")
        line = format_line("check", {"name": "build", "ok": True})
        self.assertTrue(line.startswith("CLAVE-DEV check "))
        self.assertIn('"name": "build"', line)

    def test_unknown_type_raises(self):
        with self.assertRaises(ValueError):
            format_line("nope", "x")

    def test_enabled_emitter_writes_framed_line(self):
        buf = io.StringIO()
        Emitter(enabled=True, out=buf).report({"converged": True, "rounds": 1})
        self.assertIn("CLAVE-DEV report ", buf.getvalue())
        self.assertIn('"converged": true', buf.getvalue())


class HumanModeTest(unittest.TestCase):
    """Без --protocol эмиттер обязан рассказывать человеку, что происходит.

    Раньше он просто молчал: emit() при enabled=False возвращался, и человек, запустивший
    супервайзер из терминала, минутами видел только сырой поток агента — ни раунда, ни
    результатов проверок, ни визуального прохода. Понять, где прогон и жив ли он, было неоткуда.
    """

    def test_stages_reach_the_human(self):
        human = io.StringIO()
        e = Emitter(enabled=False, human_out=human)
        e.progress("раунд 1/3: агент правит код")
        e.check({"name": "build", "ok": True})
        e.check({"name": "test", "ok": False, "detail": "2 failed"})
        e.vision({"pass": False, "issues": 3, "regressions": 1})
        e.error("базовая линия не снялась")

        out = human.getvalue()
        self.assertIn("раунд 1/3", out)
        self.assertIn("✓ build", out)
        self.assertIn("✗ test — 2 failed", out)
        self.assertIn("✗ зрение — регрессий: 1", out)
        self.assertIn("✗ базовая линия не снялась", out)

    def test_human_stages_never_touch_stdout(self):
        # В protocol-mode stdout обязан содержать ТОЛЬКО обрамлённые строки (§5), а в человеческом
        # он занят финальным отчётом. Поэтому стадии идут в stderr, а не в out.
        out, human = io.StringIO(), io.StringIO()
        Emitter(enabled=False, out=out, human_out=human).progress("тихо")
        self.assertEqual(out.getvalue(), "")
        self.assertIn("тихо", human.getvalue())

    def test_agent_output_goes_through_unadorned(self):
        # Вывод агента — его собственный текст, украшать его нечем и незачем.
        human = io.StringIO()
        Emitter(enabled=False, human_out=human).log("🅐 Claude · раунд 1 · Исполнитель")
        self.assertEqual(human.getvalue().strip(), "🅐 Claude · раунд 1 · Исполнитель")

    def test_report_and_diff_are_not_dumped_at_the_human(self):
        # Отчёт человеку печатает render_report, а дифф — это весь патч целиком.
        human = io.StringIO()
        e = Emitter(enabled=False, human_out=human)
        e.report({"converged": True})
        e.diff("--- a/src/main.rs\n+++ b/src/main.rs\n@@ …")
        self.assertEqual(human.getvalue(), "")

    def test_no_op_emitter_writes_nowhere_and_keeps_nothing(self):
        # run_loop без эмиттера и тесты не должны шуметь. И не должны КОПИТЬ: log() зовётся на
        # каждую строку агента, так что буфер-заглушка держал бы весь его вывод в памяти.
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            e = no_op_emitter()
            e.progress("ни звука")
            e.log("и это тоже")

        self.assertEqual(err.getvalue(), "")
        self.assertFalse(hasattr(e._human, "getvalue"), "сток обязан выбрасывать, а не копить")


class TuiSeesWhatTheHumanSeesTest(unittest.TestCase):
    """В TUI уезжал СЫРОЙ JSON — а рендер человеку был только в терминале.

    Живой прогон /dev показал ровно это: `✓ {"name":"test","ok":false,…}` с зелёной галочкой на
    упавшей проверке (иконка бралась по ТИПУ события), а строки «не проверено машиной» из отчёта
    тонули внутри дампа. Правило, которое не дочитывают, — декорация.

    Поэтому событие несёт готовое `human`, и рендер ОДИН — здесь. Вторая реализация в Rust уже
    разъехалась бы с этой.
    """

    def _payload(self, type_, payload):
        prefix = f"CLAVE-DEV {type_} "
        line = format_line(type_, payload)
        self.assertEqual(len(line.splitlines()), 1, "обрамлённая строка обязана быть ОДНОЙ")
        return json.loads(line[len(prefix):])

    def test_a_failed_check_carries_its_own_mark_and_reason(self):
        got = self._payload(
            "check",
            {"name": "test", "ok": False, "detail": "1 failed", "failures": ["render::footer"]},
        )
        self.assertEqual(got["human"], ["  ✗ test — 1 failed", "      · render::footer"])

    def test_the_report_carries_what_the_machine_did_not_check(self):
        got = self._payload(
            "report",
            {
                "converged": True,
                "status": "converged",
                "rounds": 1,
                "max_rounds": 3,
                "worktree": "/tmp/wt",
                "unverified": ["решена ли задача ВЕРНО — машина не проверяла"],
            },
        )
        self.assertEqual(got["human"][0], "⏺ converged — раундов: 1/3")
        self.assertIn("  ⚠ решена ли задача ВЕРНО — машина не проверяла", got["human"])


class DiffLineMatchesRealBuildDiffTest(unittest.TestCase):
    """Ключи payload берём из build_diff, а не из головы.

    Первая версия читала `files`/`patch` — таких ключей там нет и не было. Прогон показал «± правок
    нет» строкой ниже «изменено файлов: 1»: отчёт врал о собственной работе. Юнит-тест на выдуманном
    payload это пропустил, поэтому здесь — НАСТОЯЩИЙ build_diff на настоящем git.
    """

    def test_a_real_diff_is_summarised_not_denied(self):
        with tempfile.TemporaryDirectory() as tmp:
            wt = Path(tmp) / "wt"
            wt.mkdir()
            run = lambda *a: subprocess.run(  # noqa: E731
                ["git", *a], cwd=str(wt), capture_output=True, check=True
            )
            run("init", "-q")
            run("config", "user.email", "t@t")
            run("config", "user.name", "t")
            (wt / "a.rs").write_text("fn main() {}\n")
            run("add", "-A")
            run("commit", "-qm", "base")

            (wt / "a.rs").write_text("fn main() {\n    println!(\"правка\");\n}\n")
            payload = build_diff(wt, Path(tmp) / "p.patch")

            lines = human_lines("diff", payload)

        self.assertNotIn("  ± правок нет", lines, "настоящую правку отчёт назвал отсутствием правок")
        self.assertIn("1 file changed", lines[0])
        self.assertTrue(any("p.patch" in line for line in lines))

    def test_an_empty_diff_still_reads_as_empty(self):
        # Гейт, который ругается всегда, — тоже декорация.
        self.assertEqual(human_lines("diff", {"changed_files": [], "stat": ""}), ["  ± правок нет"])


class StagesCarryTheClockTest(unittest.TestCase):
    """Стадия без времени не говорит, ждать пять минут или два часа.

    Тандем с rounds=2 и effort=high законно крутит два полных цикла «исполнитель → критик» —
    замерено, около двух часов на задаче в 50 строк. Дефект не в длительности, а в молчании о ней:
    человек смотрел на неподвижное «раунд 1: агент правит код» и не мог понять, жив ли прогон.

    Часы инъекцией: тест не имеет права зависеть от настоящего времени — иначе он сам станет тем,
    что я весь день чиню.
    """

    def _emitter(self, ticks):
        human = io.StringIO()
        clock = iter(ticks).__next__
        return Emitter(enabled=False, human_out=human, clock=clock), human

    def test_a_stage_says_how_long_the_run_has_been_going(self):
        e, human = self._emitter([0.0, 125.0])  # старт, затем 2:05
        e.progress("раунд 1/3: агент правит код")

        self.assertEqual(human.getvalue().strip(), "[2:05] · раунд 1/3: агент правит код")

    def test_gates_stay_clean(self):
        # Гейты идут пачкой сразу за стадией — часы на каждой строке были бы шумом.
        e, human = self._emitter([0.0, 7.0])
        e.check({"name": "build", "ok": True})

        self.assertEqual(human.getvalue().strip(), "✓ build")

    def test_the_protocol_line_is_untouched(self):
        # TUI ведёт свой таймер; в обрамлённую строку часы лезть не должны.
        out = io.StringIO()
        Emitter(enabled=True, out=out, clock=iter([0.0, 99.0]).__next__).progress("раунд 1")

        self.assertEqual(out.getvalue().strip(), "CLAVE-DEV progress раунд 1")


class VisionDetailsTest(unittest.TestCase):
    """Зрение блокирует прогон — человек обязан видеть, ЧТО именно забраковано.

    Раньше в терминал уходили только счётчики («регрессий: 1, находок: 9»), а перечень
    проваленных required-пунктов и находки — только в промпт агента.
    """

    def test_vision_details_reach_the_human(self):
        human = io.StringIO()
        Emitter(enabled=False, human_out=human).vision(VISION_WITH_DETAILS)

        out = human.getvalue()
        self.assertIn("✗ зрение — регрессий: 1, находок: 2", out)
        self.assertIn("правая рамка срезана", out)
        self.assertIn("[high] полый курсор", out)
        self.assertIn("[low] логотип тусклый", out)

    def test_vision_without_details_prints_as_before(self):
        # Старый payload из трёх ключей — ровно одна строка, без пустых подзаголовков.
        human = io.StringIO()
        Emitter(enabled=False, human_out=human).vision({"pass": True, "issues": 0, "regressions": 0})
        self.assertEqual(human.getvalue(), "  ✓ зрение — регрессий: 0, находок: 0\n")

    def test_vision_line_stays_one_valid_json_in_protocol_mode(self):
        # Обрамлённая строка обязана остаться ОДНОЙ строкой валидного JSON: TUI читает построчно.
        out = io.StringIO()
        Emitter(enabled=True, out=out).vision(VISION_WITH_DETAILS)

        printed = out.getvalue().splitlines()
        self.assertEqual(len(printed), 1)
        prefix = "CLAVE-DEV vision "
        self.assertTrue(printed[0].startswith(prefix))
        payload = json.loads(printed[0][len(prefix):])
        self.assertEqual(payload["pass"], False)
        self.assertIsInstance(payload["issues"], int)  # тип существующих ключей менять нельзя
        self.assertEqual(payload["regressions"], 1)
        self.assertEqual(payload["failed_required"], VISION_WITH_DETAILS["failed_required"])
        self.assertEqual(payload["findings"], VISION_WITH_DETAILS["findings"])


if __name__ == "__main__":
    unittest.main()
