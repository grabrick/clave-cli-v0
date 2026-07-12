"""Прогон и разбор cargo-проверок в worktree."""
from __future__ import annotations

import re
import subprocess
from collections import namedtuple
from pathlib import Path

from .binaries import build_command

ChecksResult = namedtuple("ChecksResult", "build_ok test_failures clippy_ok fmt_ok raw")

_TEST_RESULT = re.compile(r"test result:\s*\w+\.\s*\d+ passed;\s*(\d+) failed")


def parse_test_failures(output: str) -> int:
    """Сумма 'N failed' по всем строкам 'test result:' (или 0, если ни одной нет)."""
    return sum(int(m.group(1)) for m in _TEST_RESULT.finditer(output))


def _run(worktree: Path, env: dict, args: list) -> subprocess.CompletedProcess:
    return subprocess.run(
        args, cwd=str(worktree), env=env, capture_output=True, text=True, check=False
    )


def run_checks(worktree: Path, env: dict, profile: str) -> ChecksResult:
    raw = {}
    build = _run(worktree, env, build_command(profile))
    raw["build"] = build.stdout + build.stderr
    build_ok = build.returncode == 0
    if not build_ok:
        return ChecksResult(False, 0, False, False, raw)

    test = _run(worktree, env, ["cargo", "test"])
    raw["test"] = test.stdout + test.stderr
    parsed = parse_test_failures(raw["test"])
    # cargo test упал, но счётчик не распарсился (напр. ошибка компиляции тестов) —
    # сигналим минимум одну неудачу, чтобы критерий не счёл прогон зелёным.
    test_failures = parsed if (parsed or test.returncode == 0) else 1

    clippy = _run(worktree, env, ["cargo", "clippy", "--all-targets", "--", "-D", "warnings"])
    raw["clippy"] = clippy.stdout + clippy.stderr
    clippy_ok = clippy.returncode == 0

    fmt = _run(worktree, env, ["cargo", "fmt", "--check"])
    raw["fmt"] = fmt.stdout + fmt.stderr
    fmt_ok = fmt.returncode == 0

    return ChecksResult(True, test_failures, clippy_ok, fmt_ok, raw)
