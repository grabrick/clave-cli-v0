"""Git-безопасность: preflight чистого дерева + изолированный worktree на весь прогон."""
from __future__ import annotations

import re
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


def base_sha(worktree: Path) -> str:
    """Коммит, на котором worktree создан. От него и меряется работа агента.

    Мерить от индекса нельзя: агент имеет право коммитить (и коммитит — критик тандема сам ему
    это советует, чтобы дифф сошёлся с деревом для cargo mutants). После коммита рабочее дерево
    чистое, и «изменений нет» прочиталось бы как «агент ничего не сделал».
    """
    return _git(worktree, "rev-parse", "HEAD").stdout.strip()


def remove_run_worktree(repo: Path, worktree: Path) -> None:
    _git(repo, "worktree", "remove", "--force", str(worktree))
    _git(repo, "worktree", "prune")


# Временный worktree прогона: `<tmp>/clave-dev-XXXXXXXX/wt`. Шаблон ТОЧНЫЙ и с якорями.
#
# Это не педантизм, а шрам. Убирая эти каталоги руками, я отфильтровал их как `grep 'clave-dev-'`
# — и подстрока поймала не только временные `clave-dev-a1b2c3d4`, но и мой рабочий worktree
# `clave-dev-headless`. Он был снесён вместе с мусором. Спасло только то, что ветка была
# запушена, а дерево — чистым.
#
# Шаблон, который ловит лишнее, — это не уборка, а разрушение. Поэтому: имя каталога целиком,
# суффикс из mkdtemp, и родитель обязан лежать ровно в tmp.
_RUN_DIR = re.compile(r"^clave-dev-[A-Za-z0-9_]{8}$")


def _registered(repo: Path) -> list:
    """Пути worktree, о которых знает git (--porcelain: по строке `worktree <путь>`)."""
    out = _git(repo, "worktree", "list", "--porcelain").stdout
    return [
        Path(line[len("worktree ") :].strip())
        for line in out.splitlines()
        if line.startswith("worktree ")
    ]


def stale_worktrees(repo: Path, tmp: Path, now: float, older_than_s: float = 6 * 3600) -> list:
    """Worktree от ПРОШЛЫХ прогонов. Свежие не трогаем.

    Worktree последнего прогона снимать нельзя: в нём лежит дифф, который человек и пришёл читать
    («ни коммита, ни установки не сделано — ревьюь и решай»). Течёт не он, а все предыдущие: за
    месяц их накопилось 28, и вычищал я их вручную.

    Фильтр по ВОЗРАСТУ обязателен ровно по той же причине, что и в `stale_dirs`: рядом может идти
    другой прогон, и снести его worktree — значит выдернуть код из-под живого агента.

    Каталог, которого уже нет, а запись в git осталась (машина подчистила /tmp), — тоже мусор:
    его тоже отдаём на снос, `git worktree prune` уберёт запись.
    """
    tmp = Path(tmp).resolve()
    found = []
    for path in _registered(repo):
        resolved = path.resolve()
        if resolved.name != "wt" or not _RUN_DIR.match(resolved.parent.name):
            continue
        if resolved.parent.parent != tmp:
            continue
        try:
            if now - resolved.parent.stat().st_mtime > older_than_s:
                found.append(path)
        except OSError:
            found.append(path)  # каталога нет, запись висит — мусор
    return found


def sweep_stale_worktrees(
    repo: Path, tmp: Path = None, now: float = None, older_than_s: float = 6 * 3600
) -> int:
    """Прибрать за прошлыми прогонами. Возвращает, сколько worktree снесено."""
    import shutil
    import tempfile as _tempfile
    import time as _time

    tmp = Path(tmp) if tmp is not None else Path(_tempfile.gettempdir())
    now = _time.time() if now is None else now

    swept = 0
    for path in stale_worktrees(repo, tmp, now, older_than_s):
        remove_run_worktree(repo, path)
        # Каталог прогона держит не только `wt`, но и патч для cargo mutants — сносим целиком.
        shutil.rmtree(path.parent, ignore_errors=True)
        swept += 1
    return swept


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
