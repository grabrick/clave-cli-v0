"""Оркестрация: implement → checks → observe → judge, до критерия или лимита раундов."""
from __future__ import annotations

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
from .visual_verdict import (
    BaselineUnavailableError,
    baseline_keys,
    blocking_verdict,
    consensus_verdict,
    is_infra_failure,
    verdict_passes,
)

RunConfig = namedtuple(
    "RunConfig",
    "known_good worktree repo env profile task effort rounds max_rounds scenarios "
    "vision blocking_severities terminal_profile baseline vision_samples vision_min_hits",
    # blocking_severities пуст по умолчанию: мнения судьи — свободный текст, их нельзя ни
    # вычесть по базовой линии, ни набрать консенсусом, значит гейт по ним абсолютный —
    # тот самый, из-за которого петля не сходилась никогда. См. verdict_passes.
    defaults=(None, (), None, None, 3, 2),
)
RunReport = namedtuple(
    "RunReport",
    "converged rounds_used max_rounds last_assertions known_good_version status",
    defaults=("unknown",),
)


def converged(
    checks, assertion_results, vision_verdicts=(), blocking=(), vision_required=False
) -> bool:
    """Здоровье продукта (спека §5+§6): проверки зелёные И текстовые assertions pass И все
    visual-вердикты pass. ВНИМАНИЕ: это НЕ «задача выполнена» — см. `outcome`.

    `vision_required` закрывает дыру, которую видно, только если спросить «а МОЖЕТ ли этот гейт
    провалиться?». `all([])` — это `True`, поэтому ПУСТОЙ список вердиктов читался как «зрение не
    возражает». Но пустым он бывает и когда зрение просто НЕ ОТРАБОТАЛО: бэкенд отвалился посреди
    прогона, визуального прохода не случилось вовсе. Человек просил проверку глазами, её не было —
    а ему рапортовали «сошлось». Гейт, который нельзя провалить, — не гейт, а декорация.

    Для текст-онли прогона (Фаза 1) пустой список законен: зрения там не просили.
    """
    if checks is None or not checks.build_ok:
        return False
    if vision_required and not vision_verdicts:
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


def outcome(
    changed, checks, assertion_results, vision_verdicts=(), blocking=(), vision_required=False
) -> str:
    """Итог раунда: 'no_changes' | 'converged' | 'continue'.

    КЛЮЧЕВОЕ: если агент не тронул код, зелёные проверки НИЧЕГО не доказывают — репозиторий
    был зелёным и ДО него. Считать это сходимостью значит рапортовать успех, ничего не сделав
    (ровно так и случалось: любая задача на здоровом репо «сходилась» мгновенно). Поэтому
    пустой набор изменений — отдельный честный исход `no_changes`, а не `converged`."""
    if not changed:
        return "no_changes"
    if converged(checks, assertion_results, vision_verdicts, blocking, vision_required):
        return "converged"
    return "continue"


def _emit_checks(emitter, checks) -> None:
    emitter.check({"name": "build", "ok": checks.build_ok})
    if checks.build_ok:
        emitter.check({"name": "test", "ok": checks.test_failures == 0, "detail": f"{checks.test_failures} failed"})
        emitter.check({"name": "clippy", "ok": checks.clippy_ok})
        emitter.check({"name": "fmt", "ok": checks.fmt_ok})
        emitter.check({"name": "python", "ok": getattr(checks, "py_ok", True), "detail": "юнит-набор clave-dev"})


def _short(text: str, limit: int = 120) -> str:
    """Свободный текст судьи → одна короткая строка.

    Судья пишет прозой и охотно многострочно; без схлопывания перевод строки разорвал бы и
    человеческий вывод, и одну обрамлённую строку протокола.
    """
    one_line = " ".join((text or "").split())
    return one_line if len(one_line) <= limit else one_line[: limit - 1] + "…"


def _vision_payload(verdicts, blocking) -> dict:
    """Событие vision: счётчики ПЛЮС то, ЧТО именно забраковано.

    Без перечня человек в терминале видел только «регрессий: 1, находок: 9» и не мог понять
    причину блокировки: детали уезжали лишь в промпт агента через build_visual_context.
    Порядок — как в вердиктах: сценарий за сценарием.
    """
    failed_required, findings = [], []
    for i, v in enumerate(verdicts):
        for c in v.checklist:
            if c.required and not c.passed:
                note = _short(c.note)
                failed_required.append(f"сценарий {i}: {_short(c.check)}" + (f" — {note}" if note else ""))
        for iss in v.issues:
            region = f" (region={_short(iss.region_hint, 40)})" if iss.region_hint else ""
            findings.append(f"сценарий {i}: [{iss.severity}] {_short(iss.description)}{region}")
    return {
        "pass": all(verdict_passes(v, blocking) for v in verdicts),
        "issues": sum(len(v.issues) for v in verdicts),
        "regressions": len(failed_required),
        "failed_required": failed_required,
        "findings": findings,
    }


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


def _baseline_keys(cfg, emitter, cache: list):
    """Что судья бракует на сборке ДО правок агента — по одному набору ключей на сценарий.

    Снимается ЛЕНИВО и один раз за прогон: визуальный проход дорогой, и платить за него в
    прогонах, которые до гейта не доходят (агент ничего не изменил, сборка красная), незачем.

    Базовой линии нет или она не снялась → BaselineUnavailableError. Тихо откатиться к
    абсолютному гейту нельзя: он на этом продукте не сходится НИКОГДА, и прогон молча сгорел бы
    в «не сошлось», гоняя агента чинить полый курсор неактивного окна.
    """
    if cache:
        return cache[0]
    if cfg.baseline is None:
        raise BaselineUnavailableError(
            "базовая линия рендера не задана — зрением гейтить нечем (нужен cfg.baseline)"
        )
    emitter.progress(
        f"снимаю базовую линию рендера: сборка ДО правок, {cfg.vision_samples} выборок вердикта"
    )
    per_scenario = observe_visual_all(cfg, cfg.baseline, cfg.vision_samples)
    broken = [v for samples in per_scenario for v in samples if is_infra_failure(v)]
    if broken:
        raise BaselineUnavailableError(
            "базовая линия не снялась: "
            + "; ".join(c.note for v in broken for c in v.checklist if not c.passed)
        )
    keys = [baseline_keys(samples) for samples in per_scenario]
    cache.append(keys)
    return keys


def run_loop(cfg: RunConfig, known_good_version: str, emitter=None) -> RunReport:
    emitter = emitter or no_op_emitter()
    grids = []
    assertion_results = []
    context = ""
    baseline_cache = []
    for round_i in range(1, cfg.max_rounds + 1):
        emitter.progress(f"раунд {round_i}/{cfg.max_rounds}: агент правит код")
        task = cfg.task if not context else f"{cfg.task}\n\n{context}"
        run_agent(
            cfg.known_good, cfg.worktree, task, cfg.env, cfg.effort, cfg.rounds,
            on_line=emitter.log,  # живой вывод агента: эмиттер сам знает, куда писать
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
            if cheap_green and cfg.vision is not None and not cfg.vision.available():
                # Зрение просили, а бэкенд отвалился уже на ходу (preflight ловит только старт).
                # Тихо уйти в «сошлось» нельзя: человек просил проверку глазами, её не было.
                vision_verdicts = [
                    blocking_verdict("бэкенд зрения стал недоступен посреди прогона")
                ]
            elif cheap_green and cfg.vision is not None:
                base = _baseline_keys(cfg, emitter, baseline_cache)
                emitter.progress("проверки зелёные — визуальный проход (откроется окно Terminal.app)")
                fresh_samples = observe_visual_all(cfg, fresh, cfg.vision_samples)
                # Гейт спрашивает «не сломал ли ТЫ рендер?», а не «идеален ли рендер?». Дефект,
                # который был и до правок, блокировать не может; одиночная выборка судьи —
                # подбрасывание монеты, поэтому нужен консенсус. См. consensus_verdict.
                vision_verdicts = [
                    consensus_verdict(samples, keys, cfg.vision_min_hits)
                    for samples, keys in zip(fresh_samples, base)
                ]
                emitter.vision(_vision_payload(vision_verdicts, cfg.blocking_severities))

        vision_required = cfg.vision is not None
        if (
            outcome(
                changed, checks, assertion_results, vision_verdicts,
                cfg.blocking_severities, vision_required,
            )
            == "converged"
        ):
            _emit_final(emitter, cfg, True, round_i, known_good_version, "converged")
            return RunReport(True, round_i, cfg.max_rounds, assertion_results, known_good_version, "converged")

        context = build_context(checks, grids, assertion_results)
        if vision_verdicts:
            context = context + "\n" + build_visual_context(vision_verdicts)

    _emit_final(emitter, cfg, False, cfg.max_rounds, known_good_version, "exhausted")
    return RunReport(
        False, cfg.max_rounds, cfg.max_rounds, assertion_results, known_good_version, "exhausted"
    )
