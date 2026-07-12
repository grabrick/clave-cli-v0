"""CLI супервайзера: собирает изоляцию, worktree, сценарии и гоняет петлю."""
from __future__ import annotations

import argparse
import sys
import tempfile
from pathlib import Path

from .assertions import line_matches, not_visible, visible
from .binaries import sanitized_env, snapshot_known_good
from .emit import Emitter
from .loop import RunConfig, run_loop
from .observer import Scenario
from .report import render_report
from .terminal_profile import default_profile, describe, observer_profile_mismatch
from .vision import vision_preflight
from .vision_claude import select_vision
from .visual_verdict import severities_at_or_above
from .worktree import DirtyTreeError, assert_clean, create_run_worktree

_ASSERT_FACTORIES = {"visible": visible, "not_visible": not_visible, "line_matches": line_matches}


def _parse_assert(spec: str):
    kind, _, arg = spec.partition(":")
    if kind not in _ASSERT_FACTORIES:
        raise argparse.ArgumentTypeError(f"неизвестный assert: {kind} (visible|not_visible|line_matches)")
    return _ASSERT_FACTORIES[kind](arg)


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="clave-dev", description="самопиление Clave (v1, текстовое наблюдение)")
    p.add_argument("task", help="что нужно сделать (текстовая задача агенту)")
    p.add_argument("--repo", default=".", type=Path, help="git-репозиторий Clave (чистое дерево)")
    p.add_argument("--known-good", default=str(Path.home() / ".cargo/bin/clave"), type=Path,
                   help="стабильный clave с поддержкой --run (инструмент, не объект)")
    p.add_argument("--build-profile", default="debug", choices=["debug", "release"])
    p.add_argument("--rounds", type=int, default=None, help="debate-раунды tandem внутри одного вызова")
    p.add_argument("--max-rounds", type=int, default=3, help="раундов петли супервайзера")
    p.add_argument("--effort", default=None)
    p.add_argument("--assert", dest="asserts", action="append", type=_parse_assert, default=[],
                   help="assertion над экраном, напр. 'visible:Отправка' (можно несколько)")
    p.add_argument("--vision", default=None,
                   help="бэкенд зрения (напр. claude); без него — текст-онли Фаза 1")
    p.add_argument("--terminal-profile", default=None,
                   help="имя профиля Terminal.app для среды прогона (§4)")
    p.add_argument("--severity-threshold", default="medium",
                   choices=["none", "low", "medium", "high"],
                   help="минимальный severity необязательной находки, который блокирует (§6). "
                        "'none' — блокирует только провал required-чеклиста, мнения модели "
                        "идут агенту справочно (безопасно для автономной петли)")
    p.add_argument("--protocol", default=None, choices=["clave-dev"],
                   help="типизированный вывод CLAVE-DEV для TUI (§5); без него — человеческий отчёт")
    return p


def main(argv=None) -> int:
    args = build_parser().parse_args(argv)
    repo = args.repo.resolve()

    try:
        assert_clean(repo)
    except DirtyTreeError as e:
        print(f"clave-dev: {e}", file=sys.stderr)
        return 2

    tmp = Path(tempfile.mkdtemp(prefix="clave-dev-"))
    worktree = create_run_worktree(repo, "HEAD", tmp)

    known = snapshot_known_good(args.known_good, tmp)
    env = sanitized_env(worktree)
    # Изолируем состояние инструмента: свой CLAVE_HOME и без онбординга,
    # чтобы прогон не трогал реальный конфиг пользователя.
    home = tmp / "home"
    home.mkdir(parents=True, exist_ok=True)
    env["CLAVE_HOME"] = str(home)
    env["CLAVE_SKIP_ONBOARDING"] = "1"

    scenario = Scenario(name="default", steps=[], settle_s=0.4, assertions=list(args.asserts))

    # Зрение (Фаза 2): выбирается явно. Если запрошено, но работать НЕ сможет — не стартуем
    # вовсе: молча уехать в текст-онли значит сделать не то, о чём попросили, а гнать прогон
    # ради гарантированных fail-safe вердиктов — жечь время впустую. Прогон ещё не начат,
    # так что отказ бесплатен.
    vision = select_vision(args.vision, env)
    if args.vision:
        reason = vision_preflight(vision)
        if reason:
            print(f"clave-dev: зрение запрошено, но работать не сможет — {reason}", file=sys.stderr)
            print("clave-dev: прогон НЕ начат. Перезапусти без зрения.", file=sys.stderr)
            return 2
        print(
            "clave-dev: зрение включено. На визуальном проходе откроется окно Terminal.app — "
            "не трогай в этот момент клавиатуру и мышь.",
            file=sys.stderr,
        )
        # Предупреждение, а не блок: прогон осмысленный, но вердикт будет о ЧУЖОМ рендере.
        mismatch = observer_profile_mismatch(args.terminal_profile or default_profile().theme)
        if mismatch:
            print(f"clave-dev: ⚠ {mismatch}", file=sys.stderr)
    else:
        print("clave-dev: зрение выключено — текстовое наблюдение", file=sys.stderr)

    profile = default_profile()
    if args.terminal_profile:
        profile = profile._replace(theme=args.terminal_profile)
    print(f"clave-dev: терминальная среда: {describe(profile)}", file=sys.stderr)  # §4 лог среды
    blocking = severities_at_or_above(args.severity_threshold)

    cfg = RunConfig(
        known_good=known.path,
        worktree=worktree,
        repo=repo,
        env=env,
        profile=args.build_profile,
        task=args.task,
        effort=args.effort,
        rounds=args.rounds,
        max_rounds=args.max_rounds,
        scenarios=[scenario],
        vision=vision,
        blocking_severities=blocking,
        terminal_profile=args.terminal_profile,
    )
    emitter = Emitter(enabled=(args.protocol == "clave-dev"))
    report = run_loop(cfg, known.version, emitter=emitter)
    # В protocol-mode stdout только обрамлён (§5) — человеческий отчёт не печатаем.
    if args.protocol != "clave-dev":
        print(render_report(report, repo, worktree))
    # worktree с дифом сознательно НЕ удаляется — нужен человеку для ревью (спека §7).
    return 0 if report.converged else 1


if __name__ == "__main__":
    sys.exit(main())
