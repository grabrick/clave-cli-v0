"""Гейт устойчивости: набор, падающий вразброс, ВРЁТ мутационному гейту.

Проверяем не «пропускает ли он хорошее» (это умеет и декорация), а «задерживает ли плохое»:
флейк обязан быть НАЗВАН, а невозможность проверить — обязана уронить прогон, а не тихо
превратиться в «устойчив».
"""
import unittest
from pathlib import Path

from clave_dev.context import build_flake_context
from clave_dev.flake import SuiteNotBuilt, describe, failed_tests, unstable
from clave_dev.loop import converged, outcome

OK = "test alpha ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored\n"
RED = "test alpha ... FAILED\ntest result: FAILED. 0 passed; 1 failed; 0 ignored\n"


class Checks:
    """Минимальный ChecksResult: всё зелёное, кроме того, что проверяет конкретный тест."""

    build_ok = True
    test_failures = 0
    clippy_ok = True
    fmt_ok = True
    py_ok = True


class FailedTestsTest(unittest.TestCase):
    def test_failed_tests_returns_names_not_a_count(self):
        # «1 failed» не говорит, КТО упал, а чинить надо конкретный тест.
        out = (
            "test a::b ... ok\n"
            "test c::d ... FAILED\n"
            "test e::f ... FAILED\n"
            "test result: FAILED. 1 passed; 2 failed; 0 ignored\n"
        )
        self.assertEqual(failed_tests(out), {"c::d", "e::f"})

    def test_a_green_run_names_nobody(self):
        self.assertEqual(failed_tests(OK), set())


class UnstableTest(unittest.TestCase):
    def test_a_test_that_fell_only_sometimes_is_named(self):
        # ЭТОТ тест и доказывает гейт: обезвредь `unstable` (всегда «устойчив») — и он покраснеет.
        # Один прогон зелёный, второй красный. Одиночного `cargo test` хватило бы, чтобы объявить
        # набор здоровым, — и мутационный гейт получил бы право врать.
        outputs = iter([OK, RED, OK, OK])
        got = unstable(
            Path("/нет/такого"),
            {},
            rounds=1,
            parallel=4,
            run=lambda binary, worktree, env: next(outputs),
            find=lambda worktree, env: ["/нет/такого/бинаря"],
        )
        self.assertEqual(got, ["alpha"])

    def test_a_stable_suite_is_not_slandered(self):
        got = unstable(
            Path("/нет/такого"),
            {},
            rounds=2,
            parallel=2,
            run=lambda binary, worktree, env: OK,
            find=lambda worktree, env: ["/нет/такого/бинаря"],
        )
        self.assertEqual(got, [])

    def test_a_missing_binary_is_not_silently_stable(self):
        # Нечем проверить — значит НЕ ПРОВЕРЕНО. Вернуть тут «устойчив» значило бы выдать
        # отсутствие проверки за пройденную проверку: ровно та болезнь, от которой весь гейт.
        with self.assertRaises(SuiteNotBuilt):
            unstable(
                Path("/нет/такого"),
                {},
                run=lambda binary, worktree, env: OK,
                find=lambda worktree, env: [],
            )


class ConvergedTest(unittest.TestCase):
    def test_a_flaky_suite_cannot_be_convergence(self):
        # На неустойчивом наборе не стоит НИ ОДНА цифра прогона: красный набор мутационный гейт
        # засчитывает как «мутант пойман». Замерено: 104 «выживших» против 129 настоящих.
        self.assertTrue(converged(Checks(), []))
        self.assertFalse(converged(Checks(), [], flaky_tests=["alpha"]))

    def test_outcome_does_not_call_a_flaky_round_converged(self):
        self.assertEqual(outcome(["src/x.rs"], Checks(), []), "converged")
        self.assertEqual(
            outcome(["src/x.rs"], Checks(), [], flaky_tests=["alpha"]),
            "continue",
        )


class FeedbackTest(unittest.TestCase):
    def test_describe_says_it_is_random_not_broken(self):
        line = describe(["alpha"])[0]
        self.assertIn("alpha", line)
        self.assertIn("ВРАЗБРОС", line)

    def test_context_demands_the_race_be_removed_not_the_timeout_raised(self):
        # Сказать «почини флейк» мало: агент поднимет таймаут, и гонка вернётся в первый же
        # нагруженный день. Требование обязано быть про ПРИЧИНУ.
        text = build_flake_context(["alpha"])
        self.assertIn("alpha", text)
        self.assertIn("ГОНКУ", text)
        self.assertIn("Поднять таймаут", text)
        self.assertIn("мутационный гейт", text)


if __name__ == "__main__":
    unittest.main()
