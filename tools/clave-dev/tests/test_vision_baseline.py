import unittest
from pathlib import Path

from clave_dev.vision import FakeVisionProvider
from clave_dev.visual_observer import blocking_verdict, run_visual
from clave_dev.visual_verdict import (
    INFRA_FAILURE,
    VerdictParseError,
    baseline_keys,
    consensus_verdict,
    is_infra_failure,
    verdict_passes,
)

EDGE = "Текст не касается правой границы"
GLYPHS = "Нет обрезанных глифов"


def _verdict(*items):
    """items: (check, passed) — собирает вердикт так же, как отдала бы модель."""
    return FakeVisionProvider(
        {"checklist_results": [{"check": c, "required": True, "passed": p} for c, p in items]}
    ).analyze_image(None)


class RegressionGateTest(unittest.TestCase):
    def test_defect_that_predates_the_agent_does_not_block(self):
        # Ровно то, из-за чего петля не сходилась вообще. У продукта есть дефекты, которых агент
        # не вносил, а полый курсор в НЕАКТИВНОМ окне рисует сам Terminal — фокус наблюдатель не
        # крадёт намеренно, значит в продукте это не чинится в принципе. Абсолютный гейт гонял бы
        # агента чинить курсор до исчерпания раундов.
        base = baseline_keys([_verdict((GLYPHS, False), (EDGE, True))])
        fresh = [_verdict((GLYPHS, False), (EDGE, True))] * 3

        gated = consensus_verdict(fresh, base, min_hits=2)

        self.assertTrue(verdict_passes(gated, blocking=()))
        self.assertIn("не регрессия", next(c.note for c in gated.checklist if c.check == GLYPHS))

    def test_defect_the_agent_introduced_still_blocks(self):
        # А это гейт обязан ловить: на базе правая граница была цела, после правок — срезана.
        base = baseline_keys([_verdict((GLYPHS, False), (EDGE, True))])
        fresh = [_verdict((GLYPHS, False), (EDGE, False))] * 3

        gated = consensus_verdict(fresh, base, min_hits=2)

        self.assertFalse(verdict_passes(gated, blocking=()))
        self.assertFalse(next(c.passed for c in gated.checklist if c.check == EDGE))

    def test_case_of_the_check_name_is_not_a_new_defect(self):
        # Модель возвращает имя пункта то с заглавной, то со строчной — замерено на живых
        # прогонах. Сравнение строк «как есть» не схлопнуло бы их, и дефект из базы приехал бы
        # к агенту как «новая регрессия». Фантом, гарантированно.
        base = baseline_keys([_verdict(("нет обрезанных глифов", False))])
        fresh = [_verdict(("Нет обрезанных глифов", False))] * 3

        self.assertTrue(verdict_passes(consensus_verdict(fresh, base, min_hits=2), blocking=()))

    def test_single_noisy_sample_does_not_block(self):
        # Судья невоспроизводим: одна выборка из трёх увидела дефект, две — нет. Порог совпадений
        # (min_hits) гасит одиночный шум; иначе гейт был бы подбрасыванием монеты.
        fresh = [_verdict((EDGE, False)), _verdict((EDGE, True)), _verdict((EDGE, True))]

        self.assertTrue(verdict_passes(consensus_verdict(fresh, set(), min_hits=2), blocking=()))

    def test_defect_seen_by_the_majority_blocks(self):
        fresh = [_verdict((EDGE, False)), _verdict((EDGE, False)), _verdict((EDGE, True))]

        self.assertFalse(verdict_passes(consensus_verdict(fresh, set(), min_hits=2), blocking=()))

    def test_regression_missed_by_the_first_sample_still_lands(self):
        # Шаблон вердикта берётся с первой выборки. Если бы регрессия попадала в него только
        # оттуда, шум в выборке №1 прятал бы настоящую поломку.
        fresh = [_verdict((GLYPHS, True)), _verdict((EDGE, False)), _verdict((EDGE, False))]

        gated = consensus_verdict(fresh, set(), min_hits=2)

        self.assertFalse(verdict_passes(gated, blocking=()))

    def test_broken_visual_pass_is_never_subtracted_away(self):
        # Дыра, которую легко проглядеть: сломанный проход (нет Screen Recording, пустой кадр)
        # сломан ОДИНАКОВО и на базе, и на фреше. Вычти его по базовой линии — и неработающий
        # гейт зрения тихо превратится в pass.
        broken = blocking_verdict("screencapture код 1 (нет Screen Recording?)")
        base = baseline_keys([broken])

        gated = consensus_verdict([broken, broken, broken], base, min_hits=2)

        self.assertTrue(is_infra_failure(gated))
        self.assertFalse(verdict_passes(gated, blocking=()))

    def test_infra_failure_carries_its_reason_to_the_agent(self):
        # Причина едет в note — иначе build_visual_context показал бы агенту голую константу.
        item = next(c for c in blocking_verdict("кадр пустой/чёрный").checklist if not c.passed)
        self.assertEqual(item.check, INFRA_FAILURE)
        self.assertIn("кадр пустой", item.note)



class JudgeHiccupTest(unittest.TestCase):
    """Осечка судьи — не поломка прохода.

    Живой прогон умер именно так: модель вернула синтаксически битый JSON на ОДНОЙ из трёх
    выборок базовой линии, run_visual сразу отдал блокирующий вердикт, база «не снялась» — и
    двадцать минут работы Claude и Codex ушли в мусор. Битый JSON от языковой модели — это та же
    ненадёжность, ради которой мы вообще берём несколько выборок.
    """

    def _flaky_vision(self, script):
        """script: список 'ok' | 'boom' — что вернёт очередной вызов analyze_image."""
        calls = iter(script)
        good = FakeVisionProvider(
            {"checklist_results": [{"check": EDGE, "required": True, "passed": True}]}
        )

        class Flaky(FakeVisionProvider):
            def analyze_image(self, png_path, prompt=None):
                if next(calls, "ok") == "boom":
                    raise VerdictParseError("битый JSON вердикта")
                return good.analyze_image(png_path, prompt)

        return Flaky({})

    def _run(self, vision, samples):
        return run_visual(
            42, vision, lambda cmd: 0, lambda p: b"\xff" * 100, Path("/x.png"), samples=samples
        )

    def test_one_bad_sample_is_retried_not_fatal(self):
        vs = self._run(self._flaky_vision(["boom", "ok", "ok", "ok"]), samples=3)

        self.assertEqual(len(vs), 3, "осечку надо было повторить, а не хоронить прогон")
        self.assertFalse(any(is_infra_failure(v) for v in vs))

    def test_judge_that_never_parses_still_blocks(self):
        vs = self._run(self._flaky_vision(["boom"] * 10), samples=3)

        self.assertTrue(is_infra_failure(vs[0]), "зрение мертво — это обязано блокировать")

    def test_capture_failure_is_fatal_at_once(self):
        # Поломка ЗАХВАТА — другой природы: судить нечего, повторять бессмысленно.
        vs = run_visual(
            42, self._flaky_vision([]), lambda cmd: 1, lambda p: b"\xff" * 100, Path("/x.png"),
            samples=3,
        )
        self.assertTrue(is_infra_failure(vs[0]))

    def test_too_few_samples_blocks_instead_of_going_blind(self):
        # Ловушка: выборок меньше порога совпадений → регрессия физически не наберёт min_hits,
        # и гейт молча пропустит всё. Слепой гейт хуже отсутствующего — он создаёт видимость.
        gated = consensus_verdict([_verdict((EDGE, False))], set(), min_hits=2)

        self.assertTrue(is_infra_failure(gated))
        self.assertFalse(verdict_passes(gated, blocking=()))


class ProbeShapeTest(unittest.TestCase):
    """Зонд отдаёт ОДИН вердикт, а не список.

    gui_capture_verdict стал возвращать список выборок (петле нужно несколько суждений одного
    кадра), и зонд молча сломался: probe_summary полез бы в .issues у списка. Ни один тест этого
    не поймал — run_probe считался «e2e-only».
    """

    def test_run_probe_unwraps_the_single_sample(self):
        import clave_dev.visual_observer as vo
        from clave_dev.vision_probe import probe_summary, run_probe

        good = FakeVisionProvider(
            {"checklist_results": [{"check": EDGE, "required": True, "passed": True}]}
        ).analyze_image(None)
        original = vo.gui_capture_verdict
        vo.gui_capture_verdict = lambda *a, **kw: [good]
        try:
            verdict = run_probe("/bin/true", None, None)
            summary, code = probe_summary(verdict)  # упало бы на списке
        finally:
            vo.gui_capture_verdict = original

        self.assertEqual(code, 0)
        self.assertTrue(summary["pass"])

if __name__ == "__main__":
    unittest.main()
