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


def descendant_pids(root_pid) -> dict:
    """PID → cmdline для ВСЕХ потомков процесса — по дереву ppid, а не по подстроке.

    Почему не подстрока: `cargo`/`rustc` — самые опасные сироты (жгут ядра), но в их
    командной строке нет ни `clave_dev`, ни временного пути, так что фильтр по имени их
    просто не видит. А наивный `clave-dev-` наоборот ловил каталог worktree
    (`clave-dev-headless`), то есть сам тестируемый clave, давая ложную «сироту»."""
    out = subprocess.run(
        ["ps", "-Ao", "pid=,ppid=,command="], capture_output=True, text=True
    ).stdout
    kids, info = {}, {}
    for line in out.splitlines():
        parts = line.split(None, 2)
        if len(parts) < 3:
            continue
        pid, ppid, cmd = parts
        kids.setdefault(ppid, []).append(pid)
        info[pid] = cmd
    found, stack = {}, [str(root_pid)]
    while stack:
        parent = stack.pop()
        for kid in kids.get(parent, []):
            found[kid] = info[kid][:120]
            stack.append(kid)
    return found


def still_alive(pid) -> bool:
    """Жив ли PID. Проверяем именно по PID: осиротевший процесс при reparent уходит под
    launchd и из дерева clave исчезает — обход дерева ПОСЛЕ отмены его бы не заметил."""
    return subprocess.run(["ps", "-p", str(pid)], capture_output=True).returncode == 0


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

    # Ждём именно ФАЗУ ПРОВЕРОК: отменять надо посреди cargo — осиротевший cargo/rustc
    # и есть главный риск (молча жжёт ядра). Отмена на фазе агента этого не проверяет.
    deadline = time.time() + 90
    while time.time() < deadline:
        pump(1.0)
        if "проверки" in grid():
            break
    pump(4.0)  # дать cargo реально раскрутиться

    before = descendant_pids(proc.pid)
    print(f"=== ДО ОТМЕНЫ: потомков clave (всё дерево): {len(before)} ===")
    for pid, cmd in list(before.items())[:8]:
        print(f"  {pid}  {cmd}")
    if not before:
        print("НЕ УДАЛОСЬ поймать дерево процессов — тест бессмысленен")
        proc.kill()
        return 2
    cargo_seen = any("cargo" in c or "rustc" in c for c in before.values())

    os.write(master, b"\x03")  # Ctrl+C
    pump(3.0)
    after = grid()

    time.sleep(2.5)  # время на снятие группы и пожинание
    orphans = {pid: cmd for pid, cmd in before.items() if still_alive(pid)}

    print(f"=== ПОСЛЕ Ctrl+C: выжило из тех же PID: {len(orphans)} ===")
    for pid, cmd in list(orphans.items())[:8]:
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
        "отменяли ПОСРЕДИ cargo (иначе тест слабый)": cargo_seen,
        "TUI показал остановку": "остановлен" in after or "stopped" in after,
        "НЕТ осиротевших процессов (ни cargo, ни агента)": len(orphans) == 0,
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
