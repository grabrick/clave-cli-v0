"""Запуск агента через headless `clave --run` (known-good) и разбор CLAVE-RUN."""
from __future__ import annotations

import json
import subprocess
from collections import namedtuple
from pathlib import Path
from typing import Optional

AgentResult = namedtuple("AgentResult", "status code provider usage exit_code raw")


def parse_clave_run(stdout: str, exit_code: int) -> AgentResult:
    """Берёт последнюю строку 'CLAVE-RUN <json>'; если её нет — статус 'no_marker'."""
    line = None
    for candidate in stdout.splitlines():
        if candidate.startswith("CLAVE-RUN "):
            line = candidate
    if line is None:
        return AgentResult("no_marker", None, None, None, exit_code, stdout)
    data = json.loads(line[len("CLAVE-RUN ") :])
    return AgentResult(
        status=data.get("status"),
        code=data.get("code"),
        provider=data.get("provider"),
        usage=data.get("usage"),
        exit_code=exit_code,
        raw=stdout,
    )


def run_agent(
    known_good: Path,
    worktree: Path,
    task: str,
    env: dict,
    effort: Optional[str] = None,
    rounds: Optional[int] = None,
) -> AgentResult:
    args = [str(known_good), "--run", "tandem", "--cwd", str(worktree), "--task-stdin"]
    if effort:
        args += ["--effort", effort]
    if rounds is not None:
        args += ["--rounds", str(rounds)]
    proc = subprocess.run(
        args, input=task, env=env, capture_output=True, text=True, check=False
    )
    return parse_clave_run(proc.stdout, proc.returncode)
