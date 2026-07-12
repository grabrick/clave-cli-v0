"""Оркестрация: implement → checks → observe → judge, до критерия или лимита раундов."""
from __future__ import annotations

from collections import namedtuple

from .agent import run_agent
from .assertions import structural_assertions
from .binaries import fresh_binary
from .checks import run_checks
from .context import build_context
from .observer import run_scenario

RunConfig = namedtuple(
    "RunConfig",
    "known_good worktree repo env profile task effort rounds max_rounds scenarios",
)
RunReport = namedtuple(
    "RunReport",
    "converged rounds_used max_rounds last_assertions known_good_version",
)


def converged(checks, assertion_results) -> bool:
    """Спека §5: build ок И test 0 И clippy ок И fmt ок И все assertions pass."""
    if checks is None or not checks.build_ok:
        return False
    checks_ok = checks.test_failures == 0 and checks.clippy_ok and checks.fmt_ok
    asserts_ok = all(r.passed for r in assertion_results)
    return checks_ok and asserts_ok


def run_loop(cfg: RunConfig, known_good_version: str) -> RunReport:
    grids = []
    assertion_results = []
    context = ""
    for round_i in range(1, cfg.max_rounds + 1):
        task = cfg.task if not context else f"{cfg.task}\n\n{context}"
        run_agent(cfg.known_good, cfg.worktree, task, cfg.env, cfg.effort, cfg.rounds)

        checks = run_checks(cfg.worktree, cfg.env, cfg.profile)
        grids, assertion_results = [], []
        if checks.build_ok:
            fresh = fresh_binary(cfg.worktree, cfg.profile)
            for scenario in cfg.scenarios:
                scenario = scenario._replace(
                    assertions=list(structural_assertions()) + list(scenario.assertions)
                )
                grid, results = run_scenario(fresh, cfg.env, scenario)
                grids.append(grid)
                assertion_results.extend(results)

        if converged(checks, assertion_results):
            return RunReport(True, round_i, cfg.max_rounds, assertion_results, known_good_version)
        context = build_context(checks, grids, assertion_results)

    return RunReport(False, cfg.max_rounds, cfg.max_rounds, assertion_results, known_good_version)
