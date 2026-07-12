import subprocess
import tempfile
import unittest
from pathlib import Path

from clave_dev.worktree import (
    DirtyTreeError,
    assert_clean,
    create_run_worktree,
    git_root,
    remove_run_worktree,
)


def _init_repo(path: Path) -> None:
    for args in (
        ["init", "-q"],
        ["config", "user.email", "t@t"],
        ["config", "user.name", "t"],
    ):
        subprocess.run(["git", "-C", str(path), *args], check=True)
    (path / "f.txt").write_text("hi\n")
    subprocess.run(["git", "-C", str(path), "add", "."], check=True)
    subprocess.run(["git", "-C", str(path), "commit", "-q", "-m", "init"], check=True)


class WorktreeTest(unittest.TestCase):
    def test_assert_clean_passes_on_clean_and_raises_on_dirty(self):
        with tempfile.TemporaryDirectory() as d:
            repo = Path(d)
            _init_repo(repo)
            assert_clean(repo)  # чисто — не бросает
            (repo / "f.txt").write_text("changed\n")
            with self.assertRaises(DirtyTreeError):
                assert_clean(repo)

    def test_create_and_remove_worktree(self):
        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as t:
            repo = Path(d)
            _init_repo(repo)
            wt = create_run_worktree(repo, "HEAD", Path(t))
            self.assertTrue((wt / "f.txt").is_file())
            remove_run_worktree(repo, wt)
            self.assertFalse(wt.exists())


class GitRootTest(unittest.TestCase):
    def test_git_root_from_subdir(self):
        with tempfile.TemporaryDirectory() as d:
            repo = Path(d)
            _init_repo(repo)
            sub = repo / "tools" / "x"
            sub.mkdir(parents=True)
            self.assertEqual(git_root(sub).resolve(), repo.resolve())
