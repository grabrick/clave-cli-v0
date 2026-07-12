"""CLI супервайзера: собирает изоляцию, worktree, сценарии и гоняет петлю."""
from __future__ import annotations

import argparse
import sys
import tempfile
from pathlib import Path

from .assertions import line_matches, not_visible, visible
from .binaries import sanitized_env, snapshot_known_good
from .loop import RunConfig, run_loop
from .observer import Scenario
from .report import render_report
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
    )
    report = run_loop(cfg, known.version)
    print(render_report(report, repo, worktree))
    # worktree с дифом сознательно НЕ удаляется — нужен человеку для ревью (спека §7).
    return 0 if report.converged else 1


if __name__ == "__main__":
    sys.exit(main())
