"""ЗАЩИТА ОТ ОБХОДА: правила и CI охраняют друг друга.

Три правила краснеют на каждом раунде — но пишу их я, и снести их могу тоже я. Обходов ровно три,
и все они той же природы, что уже пойманные баги: **отсутствие проверки читается как пройденная
проверка**.

1. УДАЛИТЬ WORKFLOW. Самый опасный: GitHub тогда просто ничего не запустит. Красного не будет —
   будет тишина, а тишина выглядит как успех. Это тот же `all([]) == True`, только на уровне CI.
   Ловится тем, что этот тест ЧИТАЕТ файл workflow и требует, чтобы он звал все три правила.

2. ВЫПОТРОШИТЬ ПРАВИЛО, НЕ УДАЛЯЯ. Файл на месте, имя на месте, CI находит его поимённо — а
   внутри `GATES = []`. Так и обходит уставший человек: не сносит защиту, а «временно» вынимает
   один пункт, потому что он «флаки». Ловится замком RULES.lock: правка защищённого файла роняет
   тест, пока замок не перевыпущен ОСОЗНАННО — то есть пока в диффе не появится строка «я
   поменял правила».

3. ЗАГЛУШИТЬ ШАГ CI (`continue-on-error`). Шаг падает, прогон зелёный. Ловится проверкой текста
   workflow.

Цикл замкнут: тесты требуют workflow, workflow гоняет тесты. Убрать одно, не разбудив другое,
нельзя — придётся сносить оба разом, одним большим и заметным коммитом.

Потолок честный: от редактора с правом записи защиты нет. Эти тесты не ЗАПРЕЩАЮТ обход — они
делают его ГРОМКИМ. Последний рубеж — человек, читающий дифф.
"""
import subprocess
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from clave_dev.checks import RULE_TESTS  # noqa: E402
from scripts.relock_rules import LOCK, PROTECTED, ROOT, expected, load  # noqa: E402

WORKFLOW = ROOT / ".." / ".." / ".github" / "workflows" / "dev-rules.yml"

RULE_MODULES = (
    "tests.test_gates_can_fail",
    "tests.test_unverified",
    "tests.test_no_dead_modules",
    # Сторож входит в собственный список: иначе его можно снести молча — discover просто
    # найдёт на один тест меньше и промолчит, а вместе со сторожем уйдут и замок, и
    # проверка workflow. Сторож без сторожа — это декорация.
    "tests.test_rules_are_enforced",
)


class WorkflowStillCallsTheRulesTest(unittest.TestCase):
    def test_the_workflow_exists_at_all(self):
        # Удалённый workflow не даёт красного — он не даёт НИЧЕГО. Тишина как успех.
        self.assertTrue(
            WORKFLOW.is_file(),
            "CI-файл правил пропал: GitHub просто не запустит проверку, и это будет выглядеть "
            "как её прохождение",
        )

    def test_the_workflow_names_every_rule(self):
        text = WORKFLOW.read_text()
        for module in RULE_MODULES:
            self.assertIn(
                module,
                text,
                f"CI перестал звать {module} — правило можно удалить, и никто не заметит",
            )

    def test_no_step_is_allowed_to_fail_quietly(self):
        self.assertNotIn(
            "continue-on-error",
            WORKFLOW.read_text(),
            "шаг с continue-on-error падает молча: прогон зелёный, проверки нет",
        )

    def test_the_local_gate_names_the_same_rules(self):
        # py_ok и CI должны сторожить один и тот же список. Разошлись — значит одну из проверок
        # можно обойти, не тронув другую.
        self.assertEqual(set(RULE_TESTS), set(RULE_MODULES))


class RulesAreLockedTest(unittest.TestCase):
    def test_every_protected_file_matches_the_lock(self):
        locked = load()
        self.assertTrue(locked, "RULES.lock пропал — правила больше ничем не заперты")

        drifted = [
            name
            for name, sha in expected().items()
            if locked.get(name) != sha
        ]
        self.assertFalse(
            drifted,
            "защищённые файлы изменены, а замок не перевыпущен:\n  · "
            + "\n  · ".join(drifted)
            + "\n\nЭто не запрет. Это требование сделать правку ГРОМКОЙ: перевыпусти замок\n"
            "(python3 scripts/relock_rules.py) — и в диффе появится строка «я поменял правила».",
        )

    def test_the_lock_covers_the_workflow_too(self):
        # Иначе workflow можно выпотрошить, не тронув замок.
        self.assertIn("../../.github/workflows/dev-rules.yml", PROTECTED)

    def test_the_lock_covers_itself_guard(self):
        # Сторож заперт вместе с остальными: выпотрошить его молча тоже нельзя.
        self.assertIn("tests/test_rules_are_enforced.py", PROTECTED)


class ItselfCanFailTest(unittest.TestCase):
    """Правило 1, применённое к этому сторожу: он обязан уметь провалиться."""

    def test_a_tampered_file_is_actually_caught(self):
        target = ROOT / "tests" / "test_gates_can_fail.py"
        original = target.read_bytes()
        try:
            target.write_bytes(original + "\n# выпотрошено\n".encode())
            drifted = [n for n, sha in expected().items() if load().get(n) != sha]
            self.assertIn(
                "tests/test_gates_can_fail.py",
                drifted,
                "замок не заметил подмену правила — значит он декорация",
            )
        finally:
            target.write_bytes(original)

    def test_relock_script_is_still_there(self):
        self.assertTrue((ROOT / "scripts" / "relock_rules.py").is_file())


if __name__ == "__main__":
    unittest.main()
