"""Сводка правок для показа дифа в TUI: stat + список файлов + путь к полному патчу (спека §5).

Ключевое: работа агента меряется ОТНОСИТЕЛЬНО БАЗЫ — того коммита, на котором worktree был создан.
Не относительно индекса и не относительно HEAD, потому что агент имеет право коммитить.

Так и случилось: критик тандема посоветовал исполнителю закоммитить правки, чтобы `git diff … HEAD`
сошёлся с рабочим деревом для `cargo mutants`. Совет верный. Но `git status --porcelain` видит
только НЕЗАКОММИЧЕННОЕ — дерево после коммита чистое, `changed_paths` пуст, и петля объявила бы
«агент не внёс изменений, это no-op» о работе, сделанной полностью. Патч уехал бы пустым.

Это зеркало ложной сходимости на no-op: там петля видела успех там, где ничего не делали, — здесь
увидела бы пустоту там, где сделано всё.
"""
from __future__ import annotations

import subprocess
from pathlib import Path


def _git(worktree: Path, *args: str) -> str:
    # core.quotepath=false — иначе git экранирует не-ASCII пути в кавычки с octal-escape'ами
    # («+++ "b/clave_dev/\320\277.py"»), и всё, что разбирает дифф по именам файлов, такой файл
    # ПРОПУСКАЕТ МОЛЧА. Мутационный гейт на нём слеп: код есть, проверок нет, отчёт зелёный.
    return subprocess.run(
        ["git", "-C", str(worktree), "-c", "core.quotepath=false", *args],
        capture_output=True,
        text=True,
        check=False,
    ).stdout


def changed_paths(worktree: Path, base_sha: str = None) -> list:
    """ВСЕ правки агента относительно базы: закоммиченные, незакоммиченные и новые файлы.

    `base_sha` — коммит, на котором worktree создан. Без него падаем на старое поведение
    (`status --porcelain`), но тогда коммит агента читается как «ничего не сделал».

    Новые файлы приходится добирать отдельно: `git diff` их не видит вовсе, а `ls-files --others`
    видит.
    """
    if not base_sha:
        out = _git(worktree, "status", "--porcelain")
        return sorted({line[3:].strip() for line in out.splitlines() if line.strip()})

    tracked = _git(worktree, "diff", "--name-only", base_sha)
    untracked = _git(worktree, "ls-files", "--others", "--exclude-standard")
    return sorted(
        {p.strip() for p in (tracked + "\n" + untracked).splitlines() if p.strip()}
    )


def diff_text(worktree: Path, base_sha: str = None) -> str:
    """ТЕКСТ диффа от базы.

    Отдельно от `build_diff` нарочно. `build_diff` возвращает СЛОВАРЬ (stat, список файлов, путь к
    патчу) — его payload уезжает в TUI, и класть туда весь патч нельзя. Я об этом забыл и передал
    словарь в регулярку: мутационный гейт упал `TypeError: expected string or bytes-like object`
    посреди живого прогона, а тот же промах во втором месте ронял бы `/dev` из TUI в конце КАЖДОГО
    прогона.

    Юнит-тесты не спасли: я проверил чистую функцию строкой и ни разу — место вызова. Ровно то, за
    что ругаю агента.
    """
    _git(worktree, "add", "-N", ".")  # без intent-to-add новые файлы в дифф не попадают
    against = [base_sha] if base_sha else []
    return _git(worktree, "diff", *against)


def build_diff(worktree: Path, patch_path: Path, max_files: int = 200, base_sha: str = None) -> dict:
    """Полный патч пишется в patch_path (не льётся в транскрипт); в TUI идут stat+файлы.

    Патч тоже строится ОТ БАЗЫ: иначе коммит агента исчез бы из него, и человек увидел бы пустой
    дифф под отчётом о проделанной работе.
    """
    # intent-to-add: без этого НОВЫЕ файлы агента не попали бы ни в stat, ни в патч.
    _git(worktree, "add", "-N", ".")
    against = [base_sha] if base_sha else []
    stat = _git(worktree, "diff", *against, "--stat").strip()
    files = [f for f in _git(worktree, "diff", *against, "--name-only").splitlines() if f.strip()]
    patch = _git(worktree, "diff", *against)
    Path(patch_path).write_text(patch)
    truncated = len(files) > max_files
    return {
        "stat": stat,
        "changed_files": files[:max_files],
        "patch_path": str(patch_path),
        "truncated": truncated,
    }
