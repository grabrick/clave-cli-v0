"""Агент имеет право коммитить — и его работа обязана остаться видимой.

Так и вышло на живом прогоне: критик тандема посоветовал исполнителю закоммитить правки, чтобы
`git diff … HEAD` сошёлся с рабочим деревом для `cargo mutants`. Совет верный. Но `changed_paths`
смотрел `git status --porcelain`, а тот видит только НЕЗАКОММИЧЕННОЕ: после коммита дерево чистое,
список изменений пуст, и петля объявила бы «агент не внёс изменений — это no-op» о работе,
сделанной полностью. Патч уехал бы пустым.

Зеркало ложной сходимости на no-op: там петля видела успех там, где ничего не делали; здесь
увидела бы пустоту там, где сделано всё.

Тест на настоящем git, а не на моках: вся суть — в поведении git.
"""
import subprocess
import tempfile
import unittest
from pathlib import Path

from clave_dev.diff import build_diff, changed_paths


def _git(repo, *args):
    return subprocess.run(
        ["git", "-C", str(repo), *args], capture_output=True, text=True, check=False
    ).stdout.strip()


class AgentCommitsItsWorkTest(unittest.TestCase):
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

    def tearDown(self):
        self._tmp.cleanup()

    def _agent_works_and_commits(self):
        (self.repo / "src.rs").write_text("fn base() {}\nfn added() {}\n")
        (self.repo / "new.rs").write_text("fn brand_new() {}\n")
        _git(self.repo, "add", "-A")
        _git(self.repo, "commit", "-qm", "работа агента")

    def test_committed_work_is_still_seen_as_work(self):
        self._agent_works_and_commits()

        # Так было: status --porcelain на чистом дереве → пусто → «агент ничего не сделал».
        self.assertEqual(changed_paths(self.repo), [])
        # Так стало: относительно БАЗЫ работа видна, включая новый файл.
        self.assertEqual(changed_paths(self.repo, self.base), ["new.rs", "src.rs"])

    def test_the_patch_still_carries_the_committed_work(self):
        self._agent_works_and_commits()

        with tempfile.TemporaryDirectory() as out:
            patch = Path(out) / "p.patch"
            result = build_diff(self.repo, patch, base_sha=self.base)

            text = patch.read_text()
            self.assertIn("fn added()", text, "коммит агента обязан быть в патче")
            self.assertIn("fn brand_new()", text, "новый файл — тоже")
            self.assertIn("src.rs", result["changed_files"])

    def test_uncommitted_work_is_seen_too(self):
        # Обычный случай: агент ничего не коммитил.
        (self.repo / "src.rs").write_text("fn base() {}\nfn dirty() {}\n")
        (self.repo / "untracked.rs").write_text("fn newborn() {}\n")

        self.assertEqual(changed_paths(self.repo, self.base), ["src.rs", "untracked.rs"])

    def test_an_agent_that_did_nothing_is_still_a_no_op(self):
        # Гейт обязан уметь провалиться: если правок нет — их нет, и это честный no_changes.
        self.assertEqual(changed_paths(self.repo, self.base), [])


if __name__ == "__main__":
    unittest.main()
