import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

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


@unittest.skipUnless(sys.platform == "darwin", "наблюдатель работает через Terminal.app и osascript")
@unittest.skipIf(
    os.environ.get("CLAVE_DEV_NESTED_RUN"),
    "вложенный прогон проверяет ГЕЙТЫ — GUI-тест там не доказывает ничего, а стоит окно Terminal",
)
class ObserverCleansUpAfterItselfTest(unittest.TestCase):
    """Наблюдатель убирал окна, но не каталоги.

    За прогоны в $TMPDIR натекло 97 штук: по одному `clave-dev-guihome-*` на визуальный проход и
    по одному `clave-dev-shot-*` на КАЖДЫЙ снимок. Утечка того же рода, что 24 окна Terminal, но
    незаметная: на неё не наткнёшься глазами, поэтому она и жила.

    Гоняем НАСТОЯЩИЙ `gui_capture_verdict` — то есть место вызова, а не функцию рядом. Бинарь
    подсовываем безобидный (`/bin/echo`): окно откроется и сразу закроется, снимков не будет —
    худший путь, где уборка нужнее всего. Каталог guihome создаётся и на нём, и убрать его обязаны
    тоже.

    macOS-only, и это честно, а не удобно: `gui_capture_verdict` импортирует subprocess ЛОКАЛЬНО,
    поэтому подменить его снаружи нельзя — он реально зовёт osascript. На Linux это FileNotFound,
    и в CI такой тест ронял весь набор, а канарейка правила 1 объявляла из-за него prove_gate
    декорацией. Сам модуль от пропуска не осиротеет: `run_visual` покрыт в test_visual_loop и
    test_vision_baseline, так что правило 3 по-прежнему видит его исполняемым.
    """

    def test_the_gui_home_directory_does_not_leak(self):
        import tempfile

        from clave_dev import visual_observer
        from clave_dev.terminal_profile import default_profile

        tmp = Path(tempfile.gettempdir())
        before = set(tmp.glob("clave-dev-guihome-*"))
        # Тест, который сам мусорит, — плохой тест: этот открывает НАСТОЯЩЕЕ окно Terminal.
        # Terminal убирает его не мгновенно, поэтому добиваем с несколькими попытками — иначе
        # набор оставлял бы за собой зомби, ровно тот, который мы и лечим.
        self.addCleanup(self._sweep_leftover_windows)

        visual_observer.gui_capture_verdict(
            Path("/bin/echo"),
            tmp,
            default_profile()._replace(theme="clave-dev"),
            vision=None,
            steps=(),
            settle_s=0.2,
        )

        leaked = set(tmp.glob("clave-dev-guihome-*")) - before
        self.assertFalse(leaked, f"наблюдатель оставил за собой каталоги: {leaked}")

    @staticmethod
    def _sweep_leftover_windows() -> None:
        """Добить окно теста. Мёртвым оно становится не сразу — процессы гаснут с задержкой."""
        import time

        from clave_dev import visual_observer

        for _ in range(8):  # до ~4 секунд
            closed, stuck = visual_observer.sweep_dead_windows()
            if closed or stuck:
                return
            time.sleep(0.5)


class SweepAfterCrashedRunsTest(unittest.TestCase):
    """Прибираем за прогонами, которые умерли аварийно.

    Штатно наблюдатель убирает за собой сам — но `kill -9` (а снимать прогон приходится) рвёт его
    на середине. За месяц в $TMPDIR натекло 97 каталогов, а на экране висели окна Terminal без
    вкладок, которые уже нечем закрыть.

    Уборка идёт на СТАРТЕ, а не в конце: конец может не наступить — в том и беда.
    """

    def test_only_stale_dirs_are_swept(self):
        # Возрастной фильтр — не украшение: рядом может идти ДРУГОЙ прогон, и снести его guihome
        # значит выдернуть CLAVE_HOME из-под живого clave и получить мусор вместо рендера.
        from clave_dev.visual_observer import stale_dirs

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            old = root / "clave-dev-guihome-старый"
            fresh = root / "clave-dev-shot-свежий"
            alien = root / "чужой-каталог"
            for d in (old, fresh, alien):
                d.mkdir()
            now = 1_000_000.0
            os.utime(old, (now - 7 * 3600, now - 7 * 3600))
            os.utime(fresh, (now - 60, now - 60))
            os.utime(alien, (now - 99 * 3600, now - 99 * 3600))

            found = stale_dirs(root, now, older_than_s=6 * 3600)

        self.assertEqual([p.name for p in found], ["clave-dev-guihome-старый"])

    def test_sweeping_actually_removes_them(self):
        from clave_dev.visual_observer import sweep_stale_dirs

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            old = root / "clave-dev-shot-мёртвый"
            old.mkdir()
            (old / "frame.png").write_bytes(b"x")  # непустой: rmtree, а не rmdir
            now = 1_000_000.0
            os.utime(old, (now - 7 * 3600, now - 7 * 3600))

            swept = sweep_stale_dirs(root, now)

        self.assertEqual(swept, 1)
        self.assertFalse(old.exists())

    def test_a_living_run_is_never_touched(self):
        # Окно, в котором ещё работают процессы, — чужой активный прогон. Закрыть его значит
        # сорвать чужой визуальный проход.
        from clave_dev.visual_observer import dead_windows

        def osa(script):
            if "get id of every window" in script:
                return "111, 222"
            return "/dev/ttys999" if "111" in script else "/dev/ttys000"

        with mock.patch("clave_dev.visual_observer.tab_is_empty", side_effect=lambda t: "999" in t):
            dead = dead_windows(osa)

        self.assertEqual(dead, ["111"], "закрыли бы окно живого прогона")

    def test_a_window_without_a_tab_counts_as_dead(self):
        # «Tabless zombie»: вкладки нет, tty не прочитать — закрыть его больше нечем и незачем.
        from clave_dev.visual_observer import dead_windows

        def osa(script):
            return "333" if "get id of every window" in script else ""

        self.assertEqual(dead_windows(osa), ["333"])


class SweepDoesNotLieAboutWindowsTest(unittest.TestCase):
    """Уборка не имеет права отчитаться «закрыл», не закрыв.

    Окно, потерявшее ВКЛАДКУ, не закрывается ничем: обе формы AppleScript (`close … whose id is` и
    `close … whose name contains`) рапортуют успех и оставляют его на экране. Проверено на живой
    системе, на конкретном зомби.

    Верить своей же команде тут — та же ошибка, что верить `|| true` в страже prod-ветки: успех
    возвращается всегда. Поэтому результат ПЕРЕПРОВЕРЯЕТСЯ, а застрявшее называется вслух.
    """

    def test_a_window_that_refuses_to_close_is_reported_as_stuck(self):
        from clave_dev import visual_observer

        with mock.patch.object(visual_observer, "dead_windows", return_value=["7380"]):
            closed, stuck = visual_observer.sweep_dead_windows(osa=lambda script: "")

        self.assertEqual((closed, stuck), (0, 1), "уборка соврала, что закрыла окно")

    def test_a_window_that_really_closes_is_counted(self):
        from clave_dev import visual_observer

        answers = iter([["111"], []])  # до close — есть; после — нет
        with mock.patch.object(
            visual_observer, "dead_windows", side_effect=lambda _osa: next(answers)
        ):
            closed, stuck = visual_observer.sweep_dead_windows(osa=lambda script: "")

        self.assertEqual((closed, stuck), (1, 0))

    def test_nothing_to_do_is_not_an_error(self):
        from clave_dev import visual_observer

        with mock.patch.object(visual_observer, "dead_windows", return_value=[]):
            self.assertEqual(visual_observer.sweep_dead_windows(osa=lambda s: ""), (0, 0))
