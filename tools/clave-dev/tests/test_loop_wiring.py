"""Места ВЫЗОВА, а не чистые функции.

`build_diff` возвращает СЛОВАРЬ (stat, файлы, путь к патчу) — его payload уезжает в TUI, весь патч
туда класть нельзя. Я решил, что она возвращает текст, и передал словарь в регулярку. Результат:

  * мутационный гейт упал `TypeError: expected string or bytes-like object` посреди живого прогона;
  * тот же промах во втором месте ронял бы `/dev` из TUI в конце КАЖДОГО прогона — и лежал там
    сутки, потому что все прогоны шли из CLI, где эта ветка не исполняется.

Юнит-тесты не спасли: `unproven()` и `unverified()` я проверил строкой, а место вызова — ни разу.
Ровно то, за что ругаю агента: тест на чистую функцию есть, на склейку — нет.

Поэтому здесь настоящий git и настоящий эмиттер: проверяется, что по проводам течёт то, что надо.
"""
import io
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from clave_dev.diff import build_diff, diff_text
from clave_dev.emit import Emitter
from clave_dev.loop import RunConfig, _emit_final


def _git(repo, *args):
    return subprocess.run(
        ["git", "-C", str(repo), *args], capture_output=True, text=True, check=False
    ).stdout.strip()


class DiffTextIsTextTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.repo = Path(self._tmp.name)
        _git(self.repo, "init", "-q")
        _git(self.repo, "config", "user.email", "t@t")
        _git(self.repo, "config", "user.name", "t")
        (self.repo / "src.rs").write_text("fn base() {}\n")
        _git(self.repo, "add", "-A")
        _git(self.repo, "commit", "-qm", "база")
        self.base = _git(self.repo, "rev-parse", "HEAD")
        (self.repo / "src.rs").write_text("fn base() {}\n\n#[test]\nfn added() {}\n")

    def tearDown(self):
        self._tmp.cleanup()

    def test_build_diff_returns_a_dict_and_diff_text_returns_text(self):
        # Контракт, который я перепутал. Пусть теперь он написан.
        with tempfile.TemporaryDirectory() as out:
            summary = build_diff(self.repo, Path(out) / "p.patch", base_sha=self.base)

        self.assertIsInstance(summary, dict)
        self.assertIn("patch_path", summary)
        self.assertIsInstance(diff_text(self.repo, self.base), str)
        self.assertIn("fn added()", diff_text(self.repo, self.base))


class FinalReportDoesNotCrashInProtocolModeTest(unittest.TestCase):
    """Ровно тот путь, на котором падал бы /dev из TUI.

    В человеческом режиме ветка `if emitter.enabled` не исполняется, поэтому баг и прожил сутки:
    все прогоны шли из CLI. Здесь protocol-mode включён принудительно.
    """

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.repo = Path(self._tmp.name) / "wt"
        self.repo.mkdir()
        _git(self.repo, "init", "-q")
        _git(self.repo, "config", "user.email", "t@t")
        _git(self.repo, "config", "user.name", "t")
        (self.repo / "src.rs").write_text("fn base() {}\n")
        _git(self.repo, "add", "-A")
        _git(self.repo, "commit", "-qm", "база")
        self.base = _git(self.repo, "rev-parse", "HEAD")

    def tearDown(self):
        self._tmp.cleanup()

    def _cfg(self):
        return RunConfig(
            known_good=None, worktree=self.repo, repo=self.repo, env={}, profile="debug",
            task="t", effort=None, rounds=None, max_rounds=1, scenarios=[],
            base_sha=self.base,
        )

    def test_report_carries_unverified_and_does_not_crash(self):
        (self.repo / "src.rs").write_text("fn base() {}\n\n#[test]\nfn added() {}\n")
        out = io.StringIO()

        _emit_final(Emitter(enabled=True, out=out), self._cfg(), True, 1, "clave v0", "converged")

        report = next(
            json.loads(line.partition("CLAVE-DEV report ")[2])
            for line in out.getvalue().splitlines()
            if line.startswith("CLAVE-DEV report ")
        )
        self.assertTrue(report["unverified"], "исход не имеет права ехать без «не проверено»")
        # Тест агента посчитан по ТЕКСТУ диффа — раньше сюда приезжал словарь и всё падало.
        self.assertTrue(
            any("добавлено тестов: 1" in line for line in report["unverified"]),
            report["unverified"],
        )

    def test_it_also_works_when_the_agent_committed(self):
        # Агент имеет право коммитить: дифф и счёт тестов обязаны это пережить.
        (self.repo / "src.rs").write_text("fn base() {}\n\n#[test]\nfn added() {}\n")
        _git(self.repo, "add", "-A")
        _git(self.repo, "commit", "-qm", "работа агента")
        out = io.StringIO()

        _emit_final(Emitter(enabled=True, out=out), self._cfg(), True, 1, "clave v0", "converged")

        report = next(
            json.loads(line.partition("CLAVE-DEV report ")[2])
            for line in out.getvalue().splitlines()
            if line.startswith("CLAVE-DEV report ")
        )
        self.assertTrue(
            any("добавлено тестов: 1" in line for line in report["unverified"]),
            f"коммит агента не должен прятать его работу: {report['unverified']}",
        )


if __name__ == "__main__":
    unittest.main()
