import subprocess
import tempfile
import time
import unittest
from pathlib import Path

from clave_dev.worktree import (
    DirtyTreeError,
    assert_clean,
    create_run_worktree,
    git_root,
    remove_run_worktree,
    stale_worktrees,
    sweep_stale_worktrees,
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


class StaleWorktreeSweepTest(unittest.TestCase):
    """Уборка за прошлыми прогонами. Проверяем не «сносит ли она мусор» — это умеет и `rm -rf *`,
    — а «НЕ сносит ли она лишнее»."""

    def test_a_worktree_whose_name_merely_contains_the_prefix_is_left_alone(self):
        # ШРАМ. Вычищая эти каталоги руками, я отфильтровал их как `grep 'clave-dev-'` — и
        # подстрока поймала не только временные `clave-dev-a1b2c3d4`, но и мой РАБОЧИЙ worktree
        # `clave-dev-headless`. Он был снесён вместе с мусором; спасло лишь то, что ветка была
        # запушена. Шаблон, который ловит лишнее, — это не уборка, а разрушение.
        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as t:
            repo, tmp = Path(d), Path(t)
            _init_repo(repo)

            mine = repo / "clave-dev-headless"
            subprocess.run(
                ["git", "-C", str(repo), "worktree", "add", "--detach", str(mine), "HEAD"],
                check=True, capture_output=True,
            )
            run_dir = tmp / "clave-dev-a1b2c3d4"
            run_dir.mkdir()
            old = create_run_worktree(repo, "HEAD", run_dir)

            # Час спустя: прогон заведомо мёртв (потолок — шесть часов).
            later = time.time() + 7 * 3600
            stale = stale_worktrees(repo, tmp, later)
            self.assertEqual([p.resolve() for p in stale], [old.resolve()])

            self.assertEqual(sweep_stale_worktrees(repo, tmp, later), 1)
            self.assertFalse(old.exists(), "мусорный worktree обязан быть снесён")
            self.assertTrue(mine.exists(), "рабочий worktree трогать НЕЛЬЗЯ — его имя лишь похоже")

    def test_a_fresh_worktree_is_not_swept_out_from_under_a_live_run(self):
        # Рядом может идти другой прогон. Снести его worktree — значит выдернуть код из-под
        # живого агента, поэтому фильтр по возрасту обязателен, а не для красоты.
        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as t:
            repo, tmp = Path(d), Path(t)
            _init_repo(repo)
            run_dir = tmp / "clave-dev-b2c3d4e5"
            run_dir.mkdir()
            fresh = create_run_worktree(repo, "HEAD", run_dir)

            self.assertEqual(stale_worktrees(repo, tmp, time.time()), [])
            self.assertEqual(sweep_stale_worktrees(repo, tmp, time.time()), 0)
            self.assertTrue(fresh.exists(), "свежий worktree — это, возможно, ЖИВОЙ прогон")


class GitRootTest(unittest.TestCase):
    def test_git_root_from_subdir(self):
        with tempfile.TemporaryDirectory() as d:
            repo = Path(d)
            _init_repo(repo)
            sub = repo / "tools" / "x"
            sub.mkdir(parents=True)
            self.assertEqual(git_root(sub).resolve(), repo.resolve())
