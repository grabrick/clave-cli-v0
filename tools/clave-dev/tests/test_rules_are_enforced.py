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
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from clave_dev.checks import RULE_TESTS  # noqa: E402
from scripts.relock_rules import (  # noqa: E402
    PROTECTED,
    ROOT,
    digest,
    drifted,
    expected,
    load,
)

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


class ReleaseCannotShipSelfDevTest(unittest.TestCase):
    """Тег — ВТОРОЙ путь в прод, и он обходил стража стороной.

    `release.yml` (автоген cargo-dist) срабатывает на `push: tags: <версия>` с ЛЮБОЙ ветки, а
    `prod-guard.yml` сторожит только main. Значит `git tag v0.2.0` на коммите develop собрал бы и
    ОПУБЛИКОВАЛ релиз из дерева с инструментами: пользователи, ставящие clave через `curl | sh`,
    получили бы бинарь со спрятанным входом `--run tandem` — весь пульт самопиления.

    Три релиза уже опубликованы, так что путь не теоретический. (Проверено: в их деревьях
    инструментов нет — утечки не было.)

    release.yml АВТОГЕНЕРИРУЕТСЯ. Регенерация молча выбросит стража, если он не прописан в
    dist-workspace.toml. Молчание тут — та же болезнь: отсутствие проверки читается как
    пройденная проверка. Поэтому его наличие проверяется тестом.
    """

    GUARD = ROOT / ".." / ".." / ".github" / "workflows" / "self-dev-guard.yml"
    RELEASE = ROOT / ".." / ".." / ".github" / "workflows" / "release.yml"
    DIST = ROOT / ".." / ".." / "dist-workspace.toml"

    def test_the_guard_workflow_exists(self):
        self.assertTrue(self.GUARD.is_file(), "страж релиза пропал — тег с develop опубликует пульт")

    def test_the_release_pipeline_depends_on_the_guard(self):
        text = self.RELEASE.read_text()

        self.assertIn("self-dev-guard", text, "release.yml перестал звать стража")
        # Плана мало: от него зависит ВСЁ остальное, значит стража надо ставить перед ним.
        self.assertRegex(
            text,
            r"plan:\s*\n\s*needs:\s*\n\s*- self-dev-guard",
            "`plan` больше не зависит от стража — релиз соберётся мимо него",
        )

    def test_regeneration_would_keep_the_guard(self):
        # cargo-dist перегенерирует release.yml. Без этой строки он выбросит стража молча.
        self.assertIn('plan-jobs = ["./self-dev-guard"]', self.DIST.read_text())

    def test_the_guard_checks_its_own_patterns(self):
        # Страж, способный ответить только «OK», — декорация. Канарейка обязана быть на месте.
        self.assertIn("эталонный образец", self.GUARD.read_text())


class RulesAreLockedTest(unittest.TestCase):
    def test_every_protected_file_matches_the_lock(self):
        locked = load()
        self.assertTrue(locked, "RULES.lock пропал — правила больше ничем не заперты")

        moved = drifted(locked, expected())
        self.assertFalse(
            moved,
            "защищённые файлы изменены, а замок не перевыпущен:\n  · "
            + "\n  · ".join(moved)
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
    """Правило 1, применённое к этому сторожу: он обязан уметь провалиться.

    Проверяем МЕХАНИЗМ замка, не трогая настоящие файлы. Прежняя версия доказывала работу замка
    тем, что портила реальный `test_gates_can_fail.py` и чинила его в `finally`. Мета-тесты гоняют
    набор в восьми параллельных подпроцессах, и каждый запускал этого сторожа — восемь потоков
    наперегонки правили один файл. Локально везло; CI поймал. Общего изменяемого состояния в
    тестах быть не должно, и «у меня же есть finally» — не аргумент.
    """

    def test_a_hollowed_out_rule_is_caught(self):
        # Тот самый обход: файл на месте, имя на месте, а внутри `GATES = []`.
        with tempfile.TemporaryDirectory() as tmp:
            rule = Path(tmp) / "test_gates_can_fail.py"
            rule.write_text("GATES = ['a', 'b', 'c']\n")
            locked = {"rule": digest(rule)}

            rule.write_text("GATES = []  # временно, оно флаки\n")

            self.assertEqual(
                drifted(locked, {"rule": digest(rule)}),
                ["rule"],
                "замок не заметил подмену правила — значит он декорация",
            )

    def test_an_untouched_rule_is_not_flagged(self):
        # Замок, который ругается всегда, — тоже декорация: его отключат первым.
        with tempfile.TemporaryDirectory() as tmp:
            rule = Path(tmp) / "rule.py"
            rule.write_text("GATES = ['a']\n")
            locked = {"rule": digest(rule)}

            self.assertEqual(drifted(locked, {"rule": digest(rule)}), [])

    def test_a_missing_entry_in_the_lock_counts_as_drift(self):
        # Добавил защищаемый файл, замок не перевыпустил — это тоже дрейф.
        self.assertEqual(drifted({}, {"новый": "abc"}), ["новый"])

    def test_relock_script_is_still_there(self):
        self.assertTrue((ROOT / "scripts" / "relock_rules.py").is_file())


if __name__ == "__main__":
    unittest.main()
