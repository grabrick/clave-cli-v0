"""Гоняет fresh-бинарь в pty по сценарию, снимает символьную сетку и считает assertions."""
from __future__ import annotations

import fcntl
import os
import pty
import select
import struct
import subprocess
import termios
import time
from collections import namedtuple
from pathlib import Path

from .assertions import evaluate

Scenario = namedtuple("Scenario", "name steps settle_s assertions")


def run_scenario(
    binary: Path, env: dict, scenario: Scenario, cwd: Path, cols: int = 100, rows: int = 30
):
    import pyte  # ленивый импорт: чистая логика петли тестируется без pyte

    master, slave = pty.openpty()
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
    run_env = dict(env)
    run_env.setdefault("TERM", "xterm-256color")
    run_env.setdefault("CLAVE_SKIP_ONBOARDING", "1")
    # Рендер без стенных часов: вращающийся правый слот футера иначе показывает разные
    # сегменты разной ширины в зависимости от секунды съёмки, и сравнение с эталоном
    # объявляет регрессией разницу, которой агент не вносил.
    run_env.setdefault("CLAVE_STATIC_RENDER", "1")
    # cwd обязателен, а не с дефолтом: поведение clave зависит от каталога (git-root ищется от
    # него), и наблюдаемый бинарь должен подниматься ВНУТРИ изолированного worktree — там же,
    # где его поднимает визуальный наблюдатель. Унаследованный каталог супервайзера означал бы,
    # что два гейта судят разные репозитории: assertions видят одно, зрение — другое.
    proc = subprocess.Popen(
        [str(binary)],
        stdin=slave,
        stdout=slave,
        stderr=slave,
        env=run_env,
        cwd=str(cwd),
        close_fds=True,
    )
    os.close(slave)
    screen = pyte.Screen(cols, rows)
    stream = pyte.ByteStream(screen)

    def pump(seconds):
        end = time.time() + seconds
        while time.time() < end:
            r, _, _ = select.select([master], [], [], 0.05)
            if r:
                try:
                    data = os.read(master, 65536)
                except OSError:
                    return
                if not data:
                    return
                stream.feed(data)

    pump(1.2)
    for keys, wait_s in scenario.steps:
        os.write(master, keys.encode())
        pump(wait_s)
    pump(scenario.settle_s)
    grid = [row.rstrip() for row in screen.display]

    # Бинарь мог уже умереть — например, свежая сборка паникует на старте. Для петли это
    # НОРМАЛЬНЫЙ исход: assertions обязаны его увидеть и отдать агенту как обратную связь.
    # Но писать в закрытый pty нельзя, и голый os.write ронял OSError'ом весь супервайзер —
    # то есть наблюдатель разваливался ровно тогда, когда продукт сломан сильнее всего.
    try:
        os.write(master, b"/quit\r")
        pump(0.6)
    except OSError:
        pass
    try:
        exit_code = proc.wait(timeout=3)
    except Exception:
        proc.kill()
        exit_code = -1
    try:
        os.close(master)
    except OSError:
        pass

    results = evaluate(scenario.assertions, grid, exit_code)
    return grid, results
