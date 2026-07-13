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
        lines.append(f"- python (юнит-набор clave-dev): {'ok' if getattr(checks, 'py_ok', True) else 'FAIL'}")
        for name in ("build", "test", "clippy", "fmt", "python"):
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


def build_mutation_context(mutants) -> str:
    """Блок «Тесты, которые ничего не доказывают» — обратная связь агенту.

    Формулировка важна. Сказать «покрой тестами» мало: агент допишет ещё одну тавтологию, и она
    снова пройдёт cargo. Требование должно быть про ПАДЕНИЕ: тест обязан краснеть, если функцию
    сломать. Именно это и проверяет мутация.
    """
    # Список смешанный: Rust мутирует cargo-mutants, python-половину — свой гейт. Описания у них
    # разные, и звать один describe на оба типа значит уронить петлю на AttributeError в конце
    # прогона. Ровно так уже падал /dev в _emit_final: функция проверена, МЕСТО ВЫЗОВА — нет.
    from .mutation import Mutant
    from .mutation import describe as describe_rust
    from .mutation_py import PyMutant
    from .mutation_py import describe as describe_py

    lines = [
        "## Твои тесты ничего не доказывают",
        "",
        "Я изменил твой код (мутировал) и ни один тест этого не заметил. Значит тест на эту",
        "функцию — декорация: он не умеет упасть. Зелёный `cargo test` тут ничего не значит.",
        "",
    ]
    lines += [f"- {line}" for line in describe_rust([m for m in mutants if isinstance(m, Mutant)])]
    lines += [f"- {line}" for line in describe_py([m for m in mutants if isinstance(m, PyMutant)])]
    lines += [
        "",
        "Перепиши тесты так, чтобы каждый из них ПАДАЛ при такой мутации: прибей их к конкретным",
        "значениям и границам, а не к тавтологиям вроде `assert!(x == x)` или `assert!(true)`.",
    ]
    return "\n".join(lines)
