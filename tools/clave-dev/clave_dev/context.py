"""Сборка текстового контекст-блока для следующего раунда агента (спека §3)."""
from __future__ import annotations


def build_context(checks, grids, assertion_results) -> str:
    lines = ["## Проверки"]
    if checks is None:
        lines.append("- (ещё не запускались)")
    else:
        lines.append(f"- build: {'ok' if checks.build_ok else 'FAIL'}")
        lines.append(f"- test failures: {checks.test_failures}")
        lines.append(f"- clippy: {'ok' if checks.clippy_ok else 'FAIL (-D warnings)'}")
        lines.append(f"- fmt: {'ok' if checks.fmt_ok else 'FAIL'}")
        for name in ("build", "test", "clippy", "fmt"):
            chunk = (checks.raw or {}).get(name, "")
            tail = "\n".join(chunk.splitlines()[-20:])
            if tail.strip():
                lines.append(f"\n### вывод {name} (хвост)\n{tail}")
    lines.append("\n## Экран")
    for i, grid in enumerate(grids):
        lines.append(f"\n### сценарий {i}\n" + "\n".join(grid))
    lines.append("\n## Assertions")
    for r in assertion_results:
        lines.append(f"- {'PASS' if r.passed else 'FAIL'} {r.name} {r.message}")
    return "\n".join(lines)


def build_visual_context(verdicts) -> str:
    """Блок «Визуальные дефекты» для фидбэка агенту (спека §7)."""
    lines = ["## Визуальные дефекты"]
    if not verdicts:
        lines.append("- (зрение выключено или нет вердиктов)")
    for i, v in enumerate(verdicts):
        for c in v.checklist:
            if not c.passed:
                req = "(required)" if c.required else "(optional)"
                lines.append(f"- сценарий {i}: FAIL чеклист '{c.check}' {req} {c.note}")
        for iss in v.issues:
            lines.append(f"- сценарий {i}: [{iss.severity}] {iss.description} region={iss.region_hint}")
        if v.open_critique:
            lines.append(f"- сценарий {i}: критика: {v.open_critique}")
    return "\n".join(lines)
