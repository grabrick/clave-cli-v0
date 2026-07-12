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
    on_line=None,
) -> AgentResult:
    """Гоняет агента и СТРИМИТ его вывод построчно в `on_line` по мере поступления.

    Раньше вывод забирался буферизованно (`capture_output`) — и пользователь видел минуты
    тишины, а сам ответ агента (для аналитических задач он и есть результат) никуда не шёл.
    stderr сливаем в stdout, чтобы ошибки не терялись."""
    args = [str(known_good), "--run", "tandem", "--cwd", str(worktree), "--task-stdin"]
    if effort:
        args += ["--effort", effort]
    if rounds is not None:
        args += ["--rounds", str(rounds)]

    proc = subprocess.Popen(
        args,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        env=env,
        text=True,
        bufsize=1,
    )
    proc.stdin.write(task)
    proc.stdin.close()
    lines = []
    for line in proc.stdout:
        line = line.rstrip("\n")
        lines.append(line)
        # Машинную строку контракта наружу не льём — её разбирают, а не показывают.
        if on_line and line and not line.startswith("CLAVE-RUN "):
            on_line(line)
    proc.stdout.close()
    code = proc.wait()
    return parse_clave_run("\n".join(lines), code)
