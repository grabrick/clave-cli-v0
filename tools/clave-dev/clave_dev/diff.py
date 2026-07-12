"""Сводка правок для показа дифа в TUI: stat + список файлов + путь к полному патчу (спека §5)."""
from __future__ import annotations

import subprocess
from pathlib import Path


def _git(worktree: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(worktree), *args], capture_output=True, text=True, check=False
    ).stdout


def changed_paths(worktree: Path) -> list:
    """ВСЕ изменения рабочего дерева: и правки трекнутых файлов, и НОВЫЕ файлы.

    Важно: `git diff` новых (untracked) файлов не видит вовсе — если агент создал файл,
    diff-based проверка решила бы, что изменений нет. Поэтому смотрим `status --porcelain`."""
    out = _git(worktree, "status", "--porcelain")
    return [line[3:].strip() for line in out.splitlines() if line.strip()]


def build_diff(worktree: Path, patch_path: Path, max_files: int = 200) -> dict:
    """Полный патч пишется в patch_path (не льётся в транскрипт); в TUI идут stat+файлы."""
    # intent-to-add: без этого НОВЫЕ файлы агента не попали бы ни в stat, ни в патч.
    _git(worktree, "add", "-N", ".")
    stat = _git(worktree, "diff", "--stat").strip()
    files = [f for f in _git(worktree, "diff", "--name-only").splitlines() if f.strip()]
    patch = _git(worktree, "diff")
    Path(patch_path).write_text(patch)
    truncated = len(files) > max_files
    return {
        "stat": stat,
        "changed_files": files[:max_files],
        "patch_path": str(patch_path),
        "truncated": truncated,
    }
