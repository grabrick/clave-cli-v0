#!/usr/bin/env python3
"""Проверка отмены /dev: Ctrl+C посреди живого прогона обязан снять ВСЁ дерево процессов
(python-супервайзер → clave-агент → провайдер → cargo). Осиротевший cargo молча жёг бы
машину, осиротевший агент — токены.

Ловим PID'ы детей ДО отмены, шлём Ctrl+C, и убеждаемся, что после отмены их не осталось.
"""
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

CLAVE = sys.argv[1]
REPO = sys.argv[2]
COLS, ROWS = 120, 40


def descendants() -> dict:
    """PID → cmdline для процессов супервайзера/сборки (по характерным маркерам)."""
    out = subprocess.run(
        ["ps", "-Ao", "pid=,command="], capture_output=True, text=True
    ).stdout
    found = {}
    for line in out.splitlines():
        line = line.strip()
        if not line:
            continue
        pid, _, cmd = line.partition(" ")
        if ("clave_dev" in cmd or "clave-dev-" in cmd) and "drive_dev_cancel" not in cmd:
            found[pid] = cmd[:90]
    return found


def main() -> int:
    here = Path(REPO) / "scripts" / "selfdev"
    home = Path(tempfile.mkdtemp(prefix="clave-cancel-")) / "home"
    home.mkdir(parents=True)

    master, slave = pty.openpty()
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
    env = dict(os.environ)
    env.update(
        TERM="xterm-256color",
        CLAVE_SKIP_ONBOARDING="1",
        CLAVE_HOME=str(home),
        CLAVE_CLAUDE=str(here / "mock-agent-iterate.sh"),
        CLAVE_CODEX=str(here / "mock-agent-iterate.sh"),
        PATH=os.path.expanduser("~/.cargo/bin") + ":" + env.get("PATH", ""),
    )
    env.pop("CLAVE_DEV_PYTHON", None)
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
    os.write(master, b"/dev cancel me mid-run\r")

    # Ждём, пока дерево реально поднимется (супервайзер + сборка).
    alive, deadline = {}, time.time() + 60
    while time.time() < deadline:
        pump(1.0)
        alive = descendants()
        if len(alive) >= 1 and "раунд" in grid():
            break

    print(f"=== ДО ОТМЕНЫ: живых процессов супервайзера/сборки: {len(alive)} ===")
    for pid, cmd in list(alive.items())[:6]:
        print(f"  {pid}  {cmd}")
    if not alive:
        print("НЕ УДАЛОСЬ поймать дерево процессов — тест бессмысленен")
        proc.kill()
        return 2

    os.write(master, b"\x03")  # Ctrl+C
    pump(3.0)
    after = grid()

    # Даём время на снятие группы и пожинание.
    time.sleep(2.0)
    orphans = descendants()

    print(f"=== ПОСЛЕ Ctrl+C: осталось: {len(orphans)} ===")
    for pid, cmd in list(orphans.items())[:6]:
        print(f"  СИРОТА {pid}  {cmd}")

    os.write(master, b"/quit\r")
    pump(0.6)
    try:
        proc.wait(timeout=5)
    except Exception:
        proc.kill()
    os.close(master)

    checks = {
        "прогон стартовал": "раунд" in after or "/dev cancel" in after,
        "TUI показал остановку": "остановлен" in after or "stopped" in after,
        "НЕТ осиротевших процессов": len(orphans) == 0,
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
