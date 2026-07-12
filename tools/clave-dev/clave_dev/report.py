"""Финальный отчёт: диф, версии, проверки, assertions (спека §7). Без коммита/установки."""
from __future__ import annotations

import subprocess
from pathlib import Path

from .diff import changed_paths
from .unverified import unverified


def render_report(report, repo: Path, worktree: Path) -> str:
    diff = subprocess.run(
        ["git", "-C", str(worktree), "diff"], capture_output=True, text=True
    ).stdout
    status_ru = {
        "converged": "сошлось (агент внёс правки, всё зелёное)",
        "no_changes": "АГЕНТ НЕ ВНЁС ИЗМЕНЕНИЙ — это не успех, а no-op "
                      "(зелёные проверки тут ничего не доказывают: репозиторий был зелёным и до него). "
                      "Если задача была аналитической — ответ агента в потоке выше; clave-dev создан "
                      "для правок, для анализа возьми /advisor или /plan",
        "exhausted": "лимит раундов исчерпан, до зелёного не довели",
    }.get(getattr(report, "status", "unknown"), "неизвестно")
    lines = [
        "# clave-dev: итог прогона (стоп перед финалом)",
        f"known-good: {report.known_good_version}",
        f"раундов: {report.rounds_used} / лимит {report.max_rounds}",
        f"исход: {status_ru}",
        "",
        "## Assertions (последний раунд)",
    ]
    for r in report.last_assertions:
        lines.append(f"- {'PASS' if r.passed else 'FAIL'} {r.name} {r.message}")
    # Исход никогда не едет один: «сошлось» без этого блока читается как «сделано верно».
    lines += ["", "## Не проверено машиной"]
    for line in unverified(changed_paths(worktree), diff, report.converged):
        lines.append(f"- {line}")
    lines += ["", "## Diff", diff if diff.strip() else "(нет изменений)"]
    lines += ["", f"worktree: {worktree}", "Ни коммита, ни установки не сделано — ревьюь и решай."]
    return "\n".join(lines)
