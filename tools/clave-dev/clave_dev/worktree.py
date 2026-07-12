"""Git-безопасность: preflight чистого дерева + изолированный worktree на весь прогон."""
from __future__ import annotations

import subprocess
from pathlib import Path


class DirtyTreeError(RuntimeError):
    pass


def _git(repo: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", "-C", str(repo), *args], capture_output=True, text=True, check=False
    )


def assert_clean(repo: Path) -> None:
    """v1 не поддерживает dirty-запуск: грязное дерево → abort, ничего не трогаем."""
    out = _git(repo, "status", "--porcelain").stdout
    if out.strip():
        raise DirtyTreeError(
            "рабочее дерево не чистое; v1 требует чистый чекаут (закоммить/спрячь правки)"
        )


def create_run_worktree(repo: Path, base_ref: str, tmp_dir: Path) -> Path:
    """Создаёт изолированный worktree на detached HEAD от base_ref."""
    path = Path(tmp_dir) / "wt"
    res = _git(repo, "worktree", "add", "--detach", str(path), base_ref)
    if res.returncode != 0:
        raise RuntimeError(f"git worktree add не удался: {res.stderr.strip()}")
    return path


def remove_run_worktree(repo: Path, worktree: Path) -> None:
    _git(repo, "worktree", "remove", "--force", str(worktree))
    _git(repo, "worktree", "prune")


def git_root(path: Path) -> Path:
    """Канонический git-корень для path (спека §4). Не git → RuntimeError."""
    res = subprocess.run(
        ["git", "-C", str(path), "rev-parse", "--show-toplevel"],
        capture_output=True,
        text=True,
        check=False,
    )
    if res.returncode != 0:
        raise RuntimeError(f"не git-репозиторий: {path}")
    return Path(res.stdout.strip())
