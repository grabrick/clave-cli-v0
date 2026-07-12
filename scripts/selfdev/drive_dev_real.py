#!/usr/bin/env python3
"""Тяжёлый e2e команды /dev: настоящий clave в pty запускает НАСТОЯЩИЙ внешний clave-dev
(не мок) на мок-провайдерах. Внутри — реальные cargo build/test/clippy/fmt в изолированном
worktree и pty-observer; наружу — типизированный стрим в транскрипт TUI."""
import fcntl
import os
import pty
import select
import struct
import subprocess
import sys
import tempfile
import termios
import time
from pathlib import Path

import pyte

CLAVE = sys.argv[1]      # бинарь ветки (с /dev)
REPO = sys.argv[2]       # git-корень (worktree), дерево должно быть чистым
# python супервайзера: явный путь ИЛИ "auto" — тогда CLAVE_DEV_PYTHON не задаём вовсе
# и проверяем zero-config автопоиск venv (tools/clave-dev/.venv) самим clave.
DEV_PY = sys.argv[3] if len(sys.argv) > 3 else "auto"
COLS, ROWS = 120, 44
BUDGET_S = 200


def main() -> int:
    here = Path(REPO) / "scripts" / "selfdev"
    home = Path(tempfile.mkdtemp(prefix="clave-dev-real-")) / "home"
    home.mkdir(parents=True)

    master, slave = pty.openpty()
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
    env = dict(os.environ)
    env.update(
        TERM="xterm-256color",
        CLAVE_SKIP_ONBOARDING="1",
        CLAVE_HOME=str(home),
        CLAVE_CLAUDE=str(here / "mock-claude.sh"),
        CLAVE_CODEX=str(here / "mock-codex.sh"),
        PATH=os.path.expanduser("~/.cargo/bin") + ":" + env.get("PATH", ""),
    )
    env.pop("CLAVE_DEV_PYTHON", None)
    if DEV_PY != "auto":
        env["CLAVE_DEV_PYTHON"] = DEV_PY
    proc = subprocess.Popen(
        [CLAVE], stdin=slave, stdout=slave, stderr=slave, env=env, cwd=REPO, close_fds=True
    )
    os.close(slave)
    screen = pyte.Screen(COLS, ROWS)
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

    def grid():
        return "\n".join(row.rstrip() for row in screen.display)

    pump(1.5)
    os.write(master, b"/dev prove the self-dev loop end to end\r")

    # Ждём терминального события (реальные cargo-сборки идут десятки секунд).
    seen, deadline = "", time.time() + BUDGET_S
    while time.time() < deadline:
        pump(2.0)
        seen = grid()
        if "завершил" in seen or "finished with exit code" in seen or "остановлен" in seen:
            break
    pump(1.0)
    final = grid()

    os.write(master, b"/quit\r")
    pump(0.6)
    try:
        proc.wait(timeout=5)
    except Exception:
        proc.kill()
    os.close(master)

    print("=== ТРАНСКРИПТ ===")
    print(final)
    checks = {
        "запуск /dev": "/dev prove" in final,
        "progress-строки супервайзера": "раунд" in final or "проверки" in final,
        "check-строки (build/test/clippy/fmt)": '"name"' in final or "build" in final,
        "report-строка (converged)": "converged" in final,
        "чистое завершение (код 0)": "кодом 0" in final or "exit code 0" in final,
    }
    print("=== CHECKS ===")
    ok = True
    for name, passed in checks.items():
        print(f"  [{'OK' if passed else 'FAIL'}] {name}")
        ok = ok and passed
    print("RESULT:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
