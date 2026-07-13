"""Мутационный гейт: тест агента обязан уметь провалиться.

Вывод cargo-mutants здесь настоящий — снят с живого прогона по диффу, где агенту подсунули
тест-декорацию (`assert!(true || false)`). Все четыре мутанта выжили: функция могла возвращать
ноль, возвращать единицу и вычитать вместо сложения — и все 125 тестов оставались зелёными.
"""
import unittest
from pathlib import Path

from clave_dev import mutation
from clave_dev.mutation import (
    added_functions,
    describe,
    mutants_cmd,
    mutation_preflight,
    parse_missed,
    unproven,
)

# Живой вывод cargo-mutants: агент добавил git_slot_cost и «покрыл» её тавтологией.
DECORATION_OUTPUT = """\
Found 4 mutants to test
ok       Unmutated baseline in 8s build + 0s test
MISSED   src/ui/footer.rs:268:5: replace git_slot_cost -> usize with 0 in 0s build + 0s test
MISSED   src/ui/footer.rs:268:5: replace git_slot_cost -> usize with 1 in 0s build + 0s test
MISSED   src/ui/footer.rs:271:24: replace + with - in git_slot_cost in 0s build + 0s test
MISSED   src/ui/footer.rs:271:24: replace + with * in git_slot_cost in 0s build + 0s test
4 mutants tested in 13s: 4 missed
"""

CAUGHT_OUTPUT = """\
Found 4 mutants to test
ok       Unmutated baseline in 7s build + 0s test
4 mutants tested in 11s: 4 caught
"""

# Мутант выжил, но в ЧУЖОЙ функции — агент её не писал и не обязан покрывать.
OLD_CODE_OUTPUT = """\
MISSED   src/ui/footer.rs:99:5: replace draw_footer with () in 0s build + 0s test
MISSED   src/ui/footer.rs:85:5: replace first_hint -> &str with "" in 0s build + 0s test
"""

DIFF_WITH_NEW_FN = """\
--- a/src/ui/footer.rs
+++ b/src/ui/footer.rs
+pub(crate) fn git_slot_cost(git: &str) -> usize {
+    display_width(git) + GIT_GAP
+}
+
+    #[test]
+    fn git_slot_cost_is_correct() {
+        assert!(true || false);
+    }
"""

DIFF_TOUCHING_OLD_FN = """\
--- a/src/ui/footer.rs
+++ b/src/ui/footer.rs
     pub(crate) fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
+        let git = footer_git_segment(app);
"""


class AddedFunctionsTest(unittest.TestCase):
    def test_finds_functions_the_agent_added(self):
        self.assertEqual(added_functions(DIFF_WITH_NEW_FN), {"git_slot_cost", "git_slot_cost_is_correct"})

    def test_a_touched_old_function_is_not_a_new_one(self):
        # Иначе гейт требовал бы от агента покрыть чужой draw_footer — и не проходился бы никогда.
        self.assertEqual(added_functions(DIFF_TOUCHING_OLD_FN), set())


class ParseTest(unittest.TestCase):
    def test_reads_the_function_from_both_shapes_of_mutation(self):
        missed = parse_missed(DECORATION_OUTPUT)

        self.assertEqual(len(missed), 4)
        self.assertEqual({m.function for m in missed}, {"git_slot_cost"})
        # «replace git_slot_cost -> usize with 0» и «replace + with - in git_slot_cost»
        self.assertTrue(any("-> usize with 0" in m.description for m in missed))
        self.assertTrue(any("replace + with -" in m.description for m in missed))

    def test_the_tail_of_the_line_is_not_mistaken_for_a_function(self):
        # «… in 0s build + 0s test» — если не срезать хвост, функцией станет «test».
        self.assertNotIn("test", {m.function for m in parse_missed(DECORATION_OUTPUT)})

    def test_a_clean_run_has_nothing_missed(self):
        self.assertEqual(parse_missed(CAUGHT_OUTPUT), [])


class GateTest(unittest.TestCase):
    def test_a_decoration_test_does_not_prove_the_new_code(self):
        # Тот самый обход: cargo зелёный, счётчик тестов вырос, отчёт хвалит — а доказано ничто.
        holes = unproven(DIFF_WITH_NEW_FN, DECORATION_OUTPUT)

        self.assertEqual(len(holes), 4)
        self.assertTrue(all(m.function == "git_slot_cost" for m in holes))

    def test_a_real_test_leaves_nothing_unproven(self):
        # Гейт, который всегда красный, — тоже декорация: его отключат первым.
        self.assertEqual(unproven(DIFF_WITH_NEW_FN, CAUGHT_OUTPUT), [])

    def test_old_uncovered_code_does_not_block(self):
        # 29 выживших мутантов уже есть в том, что уехало в прод. Требовать «ноль выживших»
        # значит сделать гейт непроходимым — и он кончит так же, как абсолютный гейт зрения.
        self.assertEqual(unproven(DIFF_TOUCHING_OLD_FN, OLD_CODE_OUTPUT), [])

    def test_the_agent_is_told_exactly_what_is_unproven(self):
        lines = describe(unproven(DIFF_WITH_NEW_FN, DECORATION_OUTPUT))

        self.assertTrue(all("ВЫЖИЛА" in line for line in lines))
        self.assertTrue(any("footer.rs:268" in line for line in lines))


class PreflightTest(unittest.TestCase):
    def test_the_command_runs_only_over_the_diff(self):
        # Полный мутационный прогон — часы. По диффу — минуты (замерено: 86 мутантов = 94с).
        self.assertIn("--in-diff", mutants_cmd("/tmp/x.patch"))
        self.assertIn("/tmp/x.patch", mutants_cmd("/tmp/x.patch"))

    def test_a_missing_tool_refuses_the_run_instead_of_skipping_it(self):
        # Молча пропустить гейт нельзя: отсутствие проверки прочтётся как пройденная проверка —
        # ровно та болезнь, от которой он и лечит.
        import clave_dev.mutation as mutation

        original = mutation.shutil.which
        mutation.shutil.which = lambda _name: None
        try:
            reason = mutation_preflight()
        finally:
            mutation.shutil.which = original

        self.assertIn("cargo-mutants", reason)
        self.assertIn("--no-mutants", reason)


class TestedCountTest(unittest.TestCase):
    """Сколько мутантов гейт РЕАЛЬНО проверил — от этого зависит, врать ли правилу 2.

    Нужно, чтобы отчёт не объявлял «правку не проверяет ничто» сразу после того, как гейт сказал
    «тесты агента кусаются». Живой прогон поймал ровно это: агент переписал существующий тест,
    новых `#[test]` в диффе не появилось.
    """

    def test_counts_what_the_gate_reported(self):
        self.assertEqual(mutation.tested("11 mutants tested in 19s: 11 caught"), 11)
        self.assertEqual(mutation.tested("5 mutants tested in 13s: 5 missed"), 5)
        self.assertEqual(mutation.tested("1 mutant tested in 3s: 1 caught"), 1)

    def test_a_gate_that_ran_nothing_reports_zero(self):
        # Ноль — это «гейт не работал», а не «работал и всё чисто». Молчание не равно успеху.
        self.assertEqual(mutation.tested(""), 0)
        self.assertEqual(mutation.tested("error: cargo-mutants не установлен"), 0)


class TheLoopActuallyAsksTheGateTest(unittest.TestCase):
    """Тест на МЕСТО ВЫЗОВА: петля обязана донести результат гейта до отчёта.

    Юнит-тест `tested()` останется зелёным, даже если из петли выкинуть её вызов — и правило 2
    снова начнёт кричать на проверенную правку. Ровно эта дыра (функция проверена, вызов не
    проверен) уже уронила /dev на TypeError в _emit_final.
    """

    def test_the_report_carries_the_mutation_result(self):
        from clave_dev import loop

        seen = {}

        class Spy:
            enabled = False

            def diff(self, payload):
                pass

            def report(self, payload):
                seen.update(payload)

        original_diff_text, original_changed = loop.diff_text, loop.changed_paths
        loop.diff_text = lambda *a, **k: "+fn f() {}\n"
        loop.changed_paths = lambda *a, **k: ["src/ui/footer.rs"]
        try:
            cfg = loop.RunConfig(
                known_good=Path("/x"), worktree=Path("/wt"), repo=Path("/r"), env={},
                profile="debug", task="t", effort="high", rounds=1, max_rounds=1, scenarios=(),
            )
            loop._emit_final(Spy(), cfg, True, 1, "v", "converged", mutants_tested=11)
        finally:
            loop.diff_text, loop.changed_paths = original_diff_text, original_changed

        self.assertTrue(
            any("мутационный гейт проверил 11" in line for line in seen["unverified"]),
            f"результат гейта до отчёта не доехал: {seen.get('unverified')}",
        )


if __name__ == "__main__":
    unittest.main()
