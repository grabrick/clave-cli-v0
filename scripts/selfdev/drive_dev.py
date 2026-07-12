#!/usr/bin/env python3
"""Headless-проверка команды /dev: гоняем настоящий clave в pty, подставляя внешний
clave_dev моком (через CLAVE_DEV_HOME). Наблюдаем: типизированный стрим рендерится
с иконками (• progress, ✓ check, ⏺ report), а второй /dev во время прогона → busy."""
import fcntl
import os
import pty
import select
import struct
import sys
import termios
import time
from pathlib import Path

import pyte

CLAVE = sys.argv[1]          # ./target/debug/clave
REPO = sys.argv[2]           # git-репозиторий (worktree root)
COLS, ROWS = 110, 34


def make_mock_pkg(base: Path) -> Path:
    pkg = base / "mockdev" / "clave_dev"
    pkg.mkdir(parents=True)
    (pkg / "__init__.py").write_text("")
    (pkg / "__main__.py").write_text(
        "import time\n"
        'print("CLAVE-DEV progress раунд 1: агент правит", flush=True)\n'
        'print(\'CLAVE-DEV check {"name":"build","ok":true}\', flush=True)\n'
        "time.sleep(1.6)\n"
        'print(\'CLAVE-DEV report {"converged":true,"rounds":1}\', flush=True)\n'
    )
    return base / "mockdev"


def main() -> int:
    import tempfile

    base = Path(tempfile.mkdtemp(prefix="clave-dev-drive-"))
    mockdev = make_mock_pkg(base)
    home = base / "home"
    home.mkdir()

    master, slave = pty.openpty()
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
    env = dict(os.environ)
    env.update(
        TERM="xterm-256color",
        CLAVE_SKIP_ONBOARDING="1",
        CLAVE_HOME=str(home),
        CLAVE_DEV_HOME=str(mockdev),
    )
    import subprocess

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

    def grid_text():
        return "\n".join(row.rstrip() for row in screen.display)

    pump(1.5)
    os.write(master, b"/dev fix the footer\r")
    pump(0.8)
    mid = grid_text()                 # прогресс+чек уже пришли, мок ещё спит
    os.write(master, b"/dev second\r")  # второй запуск во время busy
    pump(0.5)
    busy = grid_text()
    pump(1.8)
    final = grid_text()               # report + завершение

    os.write(master, b"/quit\r")
    pump(0.5)
    try:
        proc.wait(timeout=3)
    except Exception:
        proc.kill()
    os.close(master)

    checks = {
        "progress иконка/текст": "раунд 1" in mid,
        "check рендер (build)": ("build" in mid),
        "busy-preflight": ("уже выполняется" in busy or "already running" in busy),
        "report рендер": ('"converged"' in final or "converged" in final),
        "завершение": ("код" in final or "exit code" in final or "готово" in final),
    }
    print("=== MID ===\n" + mid)
    print("=== BUSY ===\n" + busy)
    print("=== FINAL ===\n" + final)
    print("=== CHECKS ===")
    ok = True
    for name, passed in checks.items():
        print(f"  [{'OK' if passed else 'FAIL'}] {name}")
        ok = ok and passed
    print("RESULT:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
