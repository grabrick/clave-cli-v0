"""Финальный отчёт: диф, версии, проверки, assertions (спека §7). Без коммита/установки."""
from __future__ import annotations

import subprocess
from pathlib import Path


def render_report(report, repo: Path, worktree: Path) -> str:
    diff = subprocess.run(
        ["git", "-C", str(worktree), "diff"], capture_output=True, text=True
    ).stdout
    lines = [
        "# clave-dev: итог прогона (стоп перед финалом)",
        f"known-good: {report.known_good_version}",
        f"раундов: {report.rounds_used} / лимит {report.max_rounds}",
        f"сошлось: {'да' if report.converged else 'нет'}",
        "",
        "## Assertions (последний раунд)",
    ]
    for r in report.last_assertions:
        lines.append(f"- {'PASS' if r.passed else 'FAIL'} {r.name} {r.message}")
    lines += ["", "## Diff", diff if diff.strip() else "(нет изменений)"]
    lines += ["", f"worktree: {worktree}", "Ни коммита, ни установки не сделано — ревьюь и решай."]
    return "\n".join(lines)
