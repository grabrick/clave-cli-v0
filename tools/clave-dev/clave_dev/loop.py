"""Оркестрация: implement → checks → observe → judge, до критерия или лимита раундов."""
from __future__ import annotations

from collections import namedtuple

from .agent import run_agent
from .assertions import structural_assertions
from .binaries import fresh_binary
from .checks import run_checks
from .context import build_context, build_visual_context
from .diff import build_diff
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
    "converged rounds_used max_rounds last_assertions known_good_version",
)


def converged(checks, assertion_results, vision_verdicts=(), blocking=("high", "medium")) -> bool:
    """Спека §5+§6: проверки зелёные И текстовые assertions pass И все visual-вердикты pass.
    `vision_verdicts` пустой по умолчанию — обратная совместимость с поведением Фазы 1."""
    if checks is None or not checks.build_ok:
        return False
    checks_ok = checks.test_failures == 0 and checks.clippy_ok and checks.fmt_ok
    asserts_ok = all(r.passed for r in assertion_results)
    vision_ok = all(verdict_passes(v, blocking) for v in vision_verdicts)
    return checks_ok and asserts_ok and vision_ok


def _emit_checks(emitter, checks) -> None:
    emitter.check({"name": "build", "ok": checks.build_ok})
    if checks.build_ok:
        emitter.check({"name": "test", "ok": checks.test_failures == 0, "detail": f"{checks.test_failures} failed"})
        emitter.check({"name": "clippy", "ok": checks.clippy_ok})
        emitter.check({"name": "fmt", "ok": checks.fmt_ok})


def _emit_final(emitter, cfg, converged_flag, rounds_used, known_good_version) -> None:
    # Диф считаем только когда есть кому его показать (protocol-mode); иначе — лишняя работа.
    if emitter.enabled:
        emitter.diff(build_diff(cfg.worktree, cfg.worktree / ".clave-dev.patch"))
    emitter.report({
        "converged": converged_flag,
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
        run_agent(cfg.known_good, cfg.worktree, task, cfg.env, cfg.effort, cfg.rounds)

        emitter.progress("проверки: build/test/clippy/fmt")
        checks = run_checks(cfg.worktree, cfg.env, cfg.profile)
        _emit_checks(emitter, checks)
        grids, assertion_results, vision_verdicts = [], [], []
        if checks.build_ok:
            fresh = fresh_binary(cfg.worktree, cfg.profile)
            for scenario in cfg.scenarios:
                s = scenario._replace(
                    assertions=list(structural_assertions()) + list(scenario.assertions)
                )
                grid, results = run_scenario(fresh, cfg.env, s)
                grids.append(grid)
                assertion_results.extend(results)
            if cfg.vision is not None and cfg.vision.available():
                vision_verdicts = observe_visual_all(cfg, fresh)
                emitter.vision({
                    "pass": all(verdict_passes(v, cfg.blocking_severities) for v in vision_verdicts),
                    "issues": sum(len(v.issues) for v in vision_verdicts),
                })

        if converged(checks, assertion_results, vision_verdicts, cfg.blocking_severities):
            _emit_final(emitter, cfg, True, round_i, known_good_version)
            return RunReport(True, round_i, cfg.max_rounds, assertion_results, known_good_version)
        context = build_context(checks, grids, assertion_results)
        if vision_verdicts:
            context = context + "\n" + build_visual_context(vision_verdicts)

    _emit_final(emitter, cfg, False, cfg.max_rounds, known_good_version)
    return RunReport(False, cfg.max_rounds, cfg.max_rounds, assertion_results, known_good_version)
