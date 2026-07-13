"""Мутационный гейт для PYTHON-половины: тест агента обязан уметь провалиться и здесь.

Пробел был архитектурный: Rust-код агента стерёг `cargo mutants`, а `tools/clave-dev/` — НИЧТО.
То есть в самом инструменте жила ровно та дыра, ради которой гейт и заведён: правь супервайзер,
пиши к правке `assertTrue(True)`, набор зелёный, отчёт хвалит «добавлено тестов: 1».
"""
import sys
import tempfile
import unittest
from pathlib import Path

from clave_dev.mutation_py import PyMutant, added_functions, describe, unproven

ROOT = Path(__file__).resolve().parent.parent

REAL_DIFF = """\
diff --git a/tools/clave-dev/clave_dev/loop.py b/tools/clave-dev/clave_dev/loop.py
--- a/tools/clave-dev/clave_dev/loop.py
+++ b/tools/clave-dev/clave_dev/loop.py
@@
+def padding_for(width):
+    return max(0, width - 10)
+
+async def fetch_later(url):
+    return url
+
+def _private_helper(x):
+    return x
diff --git a/tools/clave-dev/tests/test_loop.py b/tools/clave-dev/tests/test_loop.py
--- a/tools/clave-dev/tests/test_loop.py
+++ b/tools/clave-dev/tests/test_loop.py
@@
+def test_padding_shrinks_by_ten():
+    assert padding_for(40) == 30
diff --git a/src/ui/footer.rs b/src/ui/footer.rs
--- a/src/ui/footer.rs
+++ b/src/ui/footer.rs
@@
+fn rust_stays_out_of_this_gate() -> usize { 0 }
"""

# Не-ASCII имя git по умолчанию экранирует. Парсер, который этого не знает, пропускает файл МОЛЧА.
QUOTED_DIFF = (
    'diff --git "a/tools/clave-dev/clave_dev/\\320\\277.py" '
    '"b/tools/clave-dev/clave_dev/\\320\\277.py"\n'
    "--- /dev/null\n"
    '+++ "b/tools/clave-dev/clave_dev/\\320\\277.py"\n'
    "@@\n"
    "+def hidden(width):\n"
    "+    return width\n"
)


class AddedFunctionsTest(unittest.TestCase):
    def test_it_finds_what_the_agent_added_to_the_package(self):
        found = {m.function for m in added_functions(REAL_DIFF)}

        self.assertIn("padding_for", found)
        self.assertIn("fetch_later", found)  # async — тоже функция

    def test_tests_are_not_mutated(self):
        # Подменить тест и увидеть красный набор — значит доказать, что тест проверяет сам себя.
        # Бессмыслица. Мутировать надо КОД, который тесты стерегут.
        self.assertNotIn("test_padding_shrinks_by_ten", {m.function for m in added_functions(REAL_DIFF)})

    def test_rust_is_left_to_cargo_mutants(self):
        self.assertNotIn("rust_stays_out_of_this_gate", {m.function for m in added_functions(REAL_DIFF)})

    def test_private_helpers_are_covered_through_their_callers(self):
        self.assertNotIn("_private_helper", {m.function for m in added_functions(REAL_DIFF)})

    def test_the_module_path_is_resolved(self):
        module = next(m.module for m in added_functions(REAL_DIFF) if m.function == "padding_for")
        self.assertEqual(module, "clave_dev.loop")

    def test_an_escaped_path_is_not_silently_skipped(self):
        # git экранирует не-ASCII пути; пропустить такой файл — значит ослепить гейт на нём.
        found = added_functions(QUOTED_DIFF)

        self.assertEqual([m.function for m in found], ["hidden"], "файл с не-ASCII именем пропущен")
        self.assertEqual(found[0].module, "clave_dev.п")


class DescribeTest(unittest.TestCase):
    def test_it_names_the_function_and_says_what_is_wrong(self):
        # Гейт, который говорит «нет» без объяснения, рано или поздно отключают.
        lines = describe([PyMutant("clave_dev.loop", "padding_for")])

        self.assertEqual(len(lines), 1)
        self.assertIn("clave_dev.loop:padding_for", lines[0])
        self.assertIn("не смотрит на результат", lines[0])

    def test_nothing_unproven_means_nothing_to_say(self):
        self.assertEqual(describe([]), [])


class NeuterTest(unittest.TestCase):
    """Сам механизм: обезвредить функцию и посмотреть, заметит ли набор.

    Гоняем на НАСТОЯЩЕМ пакете во временной копии — это место вызова, а не функция рядом.
    """

    def _run_gate(self, test_body: str) -> list:
        with tempfile.TemporaryDirectory() as tmp:
            pkg = Path(tmp) / "clave-dev"
            (pkg / "clave_dev").mkdir(parents=True)
            (pkg / "tests").mkdir()
            (pkg / "scripts").mkdir()

            (pkg / "clave_dev" / "__init__.py").touch()
            (pkg / "tests" / "__init__.py").touch()
            (pkg / "clave_dev" / "sample.py").write_text(
                "def padding_for(width):\n    return max(0, width - 10)\n"
            )
            (pkg / "tests" / "test_sample.py").write_text(test_body)
            (pkg / "scripts" / "neuter.py").write_bytes((ROOT / "scripts" / "neuter.py").read_bytes())

            diff = (
                "diff --git a/tools/clave-dev/clave_dev/sample.py b/tools/clave-dev/clave_dev/sample.py\n"
                "+++ b/tools/clave-dev/clave_dev/sample.py\n"
                "@@\n"
                "+def padding_for(width):\n"
                "+    return max(0, width - 10)\n"
            )
            return unproven(pkg, diff, python=sys.executable)

    def test_a_decoration_is_caught(self):
        # Тест зовёт функцию и НИЧЕГО не проверяет. Именно это агент и пишет, когда ему сказали
        # «покрой тестами», а он услышал «подними счётчик».
        undone = self._run_gate(
            "import unittest\n"
            "from clave_dev.sample import padding_for\n\n"
            "class T(unittest.TestCase):\n"
            "    def test_it_runs(self):\n"
            "        padding_for(40)\n"
        )

        self.assertEqual([m.function for m in undone], ["padding_for"], "декорация прошла гейт")

    def test_a_real_test_is_not_blocked(self):
        # Гейт, который ругается всегда, — тоже декорация: его выключат первым.
        undone = self._run_gate(
            "import unittest\n"
            "from clave_dev.sample import padding_for\n\n"
            "class T(unittest.TestCase):\n"
            "    def test_it_shrinks_by_ten(self):\n"
            "        self.assertEqual(padding_for(40), 30)\n"
        )

        self.assertEqual(undone, [], "настоящий тест забракован — гейт непроходим")

    def test_a_diff_without_python_costs_nothing(self):
        # Rust-правка не должна гонять python-набор ни разу.
        self.assertEqual(unproven(ROOT, "+++ b/src/ui/footer.rs\n+fn f() {}\n"), [])


class TheLoopActuallyRunsThePythonGateTest(unittest.TestCase):
    """Тест на МЕСТО ВЫЗОВА, а не на функцию.

    Юнит-тесты выше останутся зелёными, даже если из петли выкинуть вызов `unproven_py` — и
    python-половина снова окажется без гейта. Эта дыра (функция проверена, вызов не проверен) уже
    роняла /dev на TypeError в `_emit_final`, и мутационный гейт ловил меня на ней же в Rust.
    """

    def test_the_loop_imports_and_calls_the_python_gate(self):
        source = (ROOT / "clave_dev" / "loop.py").read_text()

        self.assertIn("unproven_py", source, "петля не зовёт python-гейт")
        self.assertRegex(
            source,
            r"unproven_mutants\s*=\s*unproven_mutants\s*\+\s*unproven_py\(",
            "результат python-гейта не попадает в список, который блокирует сходимость",
        )

    def test_a_mixed_list_does_not_crash_the_agent_prompt(self):
        # Список смешанный: Mutant (Rust) и PyMutant. Один describe на оба типа уронил бы петлю
        # AttributeError'ом в конце прогона — после всей работы агента.
        from clave_dev.context import build_mutation_context
        from clave_dev.mutation import Mutant

        text = build_mutation_context([
            Mutant("src/ui/footer.rs", 42, "replace pick -> usize with 0", "pick"),
            PyMutant("clave_dev.loop", "padding_for"),
        ])

        self.assertIn("pick", text)
        self.assertIn("clave_dev.loop:padding_for", text)


if __name__ == "__main__":
    unittest.main()
