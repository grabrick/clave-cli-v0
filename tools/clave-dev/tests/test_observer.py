import tempfile
import unittest
from pathlib import Path

from clave_dev.assertions import structural_assertions
from clave_dev.observer import Scenario, run_scenario


def _fake_binary(directory: Path, script: str) -> Path:
    binary = directory / "fake-clave"
    binary.write_text(f"#!/bin/sh\n{script}\n")
    binary.chmod(0o755)
    return binary


class ObserverTest(unittest.TestCase):
    def setUp(self):
        try:
            import pyte  # noqa: F401
        except ImportError:
            self.skipTest("pyte не установлен")

    def test_scenario_runs_binary_inside_the_worktree(self):
        # Поведение clave зависит от каталога: git-root он ищет от cwd, и на этом стоит /dev.
        # Наблюдаемый бинарь обязан подниматься ВНУТРИ изолированного worktree — там же, где его
        # поднимает визуальный наблюдатель. Пока cwd не задавался, pty-наблюдатель наследовал
        # каталог супервайзера, и два гейта судили разные репозитории: assertions видели рабочее
        # дерево разработчика, зрение — worktree агента. Приёмка могла пройти по ложной причине.
        with tempfile.TemporaryDirectory() as tmp:
            worktree = Path(tmp).resolve() / "wt"
            worktree.mkdir()
            binary = _fake_binary(Path(tmp), "pwd")

            grid, _ = run_scenario(
                binary,
                {},
                Scenario(name="cwd", steps=(), settle_s=0.4, assertions=()),
                worktree,
                cols=160,
                rows=10,
            )

            self.assertIn(str(worktree), "\n".join(grid))

    def test_binary_dying_on_start_is_a_failed_assertion_not_a_crash(self):
        # Свежая сборка, которая паникует на старте, — самый важный случай для петли: именно его
        # надо вернуть агенту обратной связью, чтобы он чинил. Наблюдатель обязан пережить мёртвый
        # pty и отдать провал. Раньше он писал в закрытый pty голым os.write и ронял OSError'ом
        # весь супервайзер — то есть разваливался ровно тогда, когда продукт сломан сильнее всего.
        with tempfile.TemporaryDirectory() as tmp:
            binary = _fake_binary(Path(tmp), "echo 'thread panicked at src/main.rs'\nexit 101")

            _, results = run_scenario(
                binary,
                {},
                Scenario(
                    name="panic",
                    steps=(),
                    settle_s=0.3,
                    assertions=tuple(structural_assertions()),
                ),
                Path(tmp).resolve(),
            )

            by_name = {r.name: r for r in results}
            self.assertFalse(by_name["clean_exit"].passed)
            self.assertIn("101", by_name["clean_exit"].message)

    def test_scenario_grid_carries_what_the_binary_drew(self):
        # Сетка — это то, по чему считаются assertions. Если бы она приходила пустой, любой
        # `visible:` молча проходил бы мимо цели.
        with tempfile.TemporaryDirectory() as tmp:
            binary = _fake_binary(Path(tmp), "echo 'готов к работе'")

            grid, _ = run_scenario(
                binary,
                {},
                Scenario(name="draw", steps=(), settle_s=0.4, assertions=()),
                Path(tmp).resolve(),
            )

            self.assertIn("готов к работе", "\n".join(grid))


if __name__ == "__main__":
    unittest.main()
