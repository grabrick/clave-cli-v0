"""Оркестрация: implement → checks → observe → judge, до критерия или лимита раундов."""
from __future__ import annotations

import sys
from collections import namedtuple

from .agent import run_agent
from .assertions import structural_assertions
from .binaries import fresh_binary
from .checks import run_checks
from .context import build_context, build_visual_context
from .diff import build_diff, changed_paths
from .emit import no_op_emitter
from .observer import run_scenario
from .visual_observer import observe_visual_all
from .visual_verdict import verdict_passes

RunConfig = namedtuple(
    "RunConfig",
    "known_good worktree repo env profile task effort rounds max_rounds scenarios "
    "vision blocking_severities terminal_profile",
    defaults=(None, ("high", "medium"), None),
)
RunReport = namedtuple(
    "RunReport",
    "converged rounds_used max_rounds last_assertions known_good_version status",
    defaults=("unknown",),
)


def converged(checks, assertion_results, vision_verdicts=(), blocking=("high", "medium")) -> bool:
    """Здоровье продукта (спека §5+§6): проверки зелёные И текстовые assertions pass И все
    visual-вердикты pass. ВНИМАНИЕ: это НЕ «задача выполнена» — см. `outcome`."""
    if checks is None or not checks.build_ok:
        return False
    # py_ok — юнит-набор самого clave-dev: без него правка в Python «сходилась» бы на
    # зелёном Rust, ничего про себя не доказав.
    checks_ok = (
        checks.test_failures == 0
        and checks.clippy_ok
        and checks.fmt_ok
        and getattr(checks, "py_ok", True)
    )
    asserts_ok = all(r.passed for r in assertion_results)
    vision_ok = all(verdict_passes(v, blocking) for v in vision_verdicts)
    return checks_ok and asserts_ok and vision_ok


def outcome(changed, checks, assertion_results, vision_verdicts=(), blocking=("high", "medium")) -> str:
    """Итог раунда: 'no_changes' | 'converged' | 'continue'.

    КЛЮЧЕВОЕ: если агент не тронул код, зелёные проверки НИЧЕГО не доказывают — репозиторий
    был зелёным и ДО него. Считать это сходимостью значит рапортовать успех, ничего не сделав
    (ровно так и случалось: любая задача на здоровом репо «сходилась» мгновенно). Поэтому
    пустой набор изменений — отдельный честный исход `no_changes`, а не `converged`."""
    if not changed:
        return "no_changes"
    if converged(checks, assertion_results, vision_verdicts, blocking):
        return "converged"
    return "continue"


def _relay(emitter, line: str) -> None:
    """Живой вывод агента наружу: в protocol-mode обрамлённым `log`, иначе — в stderr
    (stdout в protocol-mode обязан оставаться только обрамлённым, спека §5)."""
    if emitter.enabled:
        emitter.log(line)
    else:
        print(line, file=sys.stderr)


def _emit_checks(emitter, checks) -> None:
    emitter.check({"name": "build", "ok": checks.build_ok})
    if checks.build_ok:
        emitter.check({"name": "test", "ok": checks.test_failures == 0, "detail": f"{checks.test_failures} failed"})
        emitter.check({"name": "clippy", "ok": checks.clippy_ok})
        emitter.check({"name": "fmt", "ok": checks.fmt_ok})
        emitter.check({"name": "python", "ok": getattr(checks, "py_ok", True), "detail": "юнит-набор clave-dev"})


def _emit_final(emitter, cfg, converged_flag, rounds_used, known_good_version, status) -> None:
    # Диф считаем только когда есть кому его показать (protocol-mode); иначе — лишняя работа.
    # Патч пишем ВНЕ worktree: внутри он попал бы в собственный диф (build_diff делает
    # `git add -N .`, чтобы видеть новые файлы) и в changed_paths.
    if emitter.enabled:
        emitter.diff(build_diff(cfg.worktree, cfg.worktree.parent / "clave-dev.patch"))
    emitter.report({
        "converged": converged_flag,
        "status": status,
        "rounds": rounds_used,
        "max_rounds": cfg.max_rounds,
        "worktree": str(cfg.worktree),
        "known_good": known_good_version,
    })


def run_loop(cfg: RunConfig, known_good_version: str, emitter=None) -> RunReport:
    emitter = emitter or no_op_emitter()
    grids = []
    assertion_results = []
    context = ""
    for round_i in range(1, cfg.max_rounds + 1):
        emitter.progress(f"раунд {round_i}/{cfg.max_rounds}: агент правит код")
        task = cfg.task if not context else f"{cfg.task}\n\n{context}"
        run_agent(
            cfg.known_good, cfg.worktree, task, cfg.env, cfg.effort, cfg.rounds,
            on_line=lambda line: _relay(emitter, line),
        )

        # Агент не тронул код → проверки гонять незачем (проверять нечего) и объявлять
        # сходимость нельзя (см. `outcome`). Честно останавливаемся: ответ агента уже
        # ушёл наружу стримом — для аналитических задач он и есть результат.
        changed = changed_paths(cfg.worktree)
        if not changed:
            emitter.progress(
                "агент не внёс изменений в код — это no-op, а не сходимость. "
                "Ответ агента выше. /dev нужен для ПРАВОК; для анализа/диагностики — /advisor или /plan"
            )
            _emit_final(emitter, cfg, False, round_i, known_good_version, "no_changes")
            return RunReport(False, round_i, cfg.max_rounds, [], known_good_version, "no_changes")

        emitter.progress(f"изменено файлов: {len(changed)} · проверки build/test/clippy/fmt")
        checks = run_checks(cfg.worktree, cfg.env, cfg.profile)
        _emit_checks(emitter, checks)
        grids, assertion_results, vision_verdicts = [], [], []
        if checks.build_ok:
            fresh = fresh_binary(cfg.worktree, cfg.profile)
            for scenario in cfg.scenarios:
                s = scenario._replace(
                    assertions=list(structural_assertions()) + list(scenario.assertions)
                )
                grid, results = run_scenario(fresh, cfg.env, s, cfg.worktree)
                grids.append(grid)
                assertion_results.extend(results)
            # Зрение — ПОСЛЕДНИЙ гейт. Визуальный проход тяжёлый (окно Terminal.app, снимок,
            # вызов зрячей модели), поэтому запускаем его только когда дешёвые проверки уже
            # зелёные: смотреть глазами на сломанную сборку незачем и дорого.
            cheap_green = converged(checks, assertion_results)
            if cheap_green and cfg.vision is not None and cfg.vision.available():
                emitter.progress("проверки зелёные — визуальный проход (откроется окно Terminal.app)")
                vision_verdicts = observe_visual_all(cfg, fresh)
                emitter.vision({
                    "pass": all(verdict_passes(v, cfg.blocking_severities) for v in vision_verdicts),
                    "issues": sum(len(v.issues) for v in vision_verdicts),
                })

        if outcome(changed, checks, assertion_results, vision_verdicts, cfg.blocking_severities) == "converged":
            _emit_final(emitter, cfg, True, round_i, known_good_version, "converged")
            return RunReport(True, round_i, cfg.max_rounds, assertion_results, known_good_version, "converged")

        context = build_context(checks, grids, assertion_results)
        if vision_verdicts:
            context = context + "\n" + build_visual_context(vision_verdicts)

    _emit_final(emitter, cfg, False, cfg.max_rounds, known_good_version, "exhausted")
    return RunReport(
        False, cfg.max_rounds, cfg.max_rounds, assertion_results, known_good_version, "exhausted"
    )
