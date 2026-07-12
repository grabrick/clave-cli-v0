# clave-dev — Plan 2: внешний супервайзер (Implementation Plan)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Внешний Python-супервайзер `clave-dev`, который по текстовой задаче автономно доводит репозиторий Clave до зелёного: агент (через `clave --run tandem`) правит код в изолированном worktree, супервайзер гоняет build/test/clippy/fmt и наблюдает продукт через pty с assertions, и цикл повторяется до формального критерия или лимита раундов — затем стоп с дифом и отчётом, без коммита/установки.

**Architecture:** Control plane снаружи бинаря (Python), см. спеку `docs/design/2026-07-12-clave-dev-supervisor.md`. Модули: `binaries` (изоляция known-good/fresh + PATH), `worktree` (preflight+create+remove), `checks` (cargo+парсинг), `agent` (`clave --run` + `CLAVE-RUN` json), `observer`/`assertions` (pty-сетка + машинные предикаты), `loop`/`report`/`context` (петля, критерий, отчёт), `cli`.

**Tech Stack:** Python 3 stdlib + `pyte` (для observer, как в `scripts/render_check.py`); тесты — stdlib `unittest`. Никаких других зависимостей.

## Global Constraints

- Известный-хороший (known-good) `clave` и свежий (fresh) `clave` — **жёстко разделены** (спека §6): known-good по абсолютному пути + копия в temp + лог версии; fresh = `<worktree>/target/<profile>/clave`, только в observer; дочерние процессы с PATH без `target/debug`, `target/release`, `.`.
- `build_profile` (по умолчанию `debug`) — ЕДИНЫЙ источник команды сборки и пути fresh-бинаря (спека §4).
- Критерий останова (спека §5): build ок И test 0 падений И `cargo clippy --all-targets -- -D warnings` код 0 И `cargo fmt --check` чисто И все active-assertions `pass`.
- Active-assertions — supervisor-owned, неизменны в прогоне; агент не может их ослаблять (спека §5).
- git-безопасность (спека §7): preflight требует чистое дерево (иначе abort); весь прогон в отдельном `git worktree`; в конце отчёт с дифом; никакого `checkout .` в пользовательском дереве.
- Внутри цикла — никаких коммитов/установок (спека §1). Финал — стоп на ревью.
- Все процессы вызываются по абсолютному пути; `clave --run` через known-good.
- Пакет: `tools/clave-dev/clave_dev/`; тесты: `tools/clave-dev/tests/`; запуск: `python3 -m clave_dev` (из `tools/clave-dev`).

---

### Task 1: Изоляция бинарей и PATH (`binaries.py`)

**Files:**
- Create: `tools/clave-dev/clave_dev/__init__.py` (пустой)
- Create: `tools/clave-dev/clave_dev/binaries.py`
- Create: `tools/clave-dev/tests/__init__.py` (пустой)
- Create: `tools/clave-dev/tests/test_binaries.py`

**Interfaces:**
- Produces: `PROFILE_DIRS = {"debug": "debug", "release": "release"}`; `build_command(profile) -> list[str]`; `fresh_binary(worktree: Path, profile: str) -> Path`; `sanitized_env(worktree: Path, base_env: Mapping|None=None) -> dict`; `snapshot_known_good(known_good: Path, tmp_dir: Path) -> KnownGood` где `KnownGood = namedtuple("KnownGood", "path version")`.

- [ ] **Step 1: Написать `binaries.py` и падающие тесты**

`tools/clave-dev/clave_dev/binaries.py`:
```python
"""Разделение и изоляция бинарей: known-good (инструмент) vs fresh (объект)."""
from __future__ import annotations

import os
import shutil
import subprocess
from collections import namedtuple
from pathlib import Path
from typing import Mapping, Optional

PROFILE_DIRS = {"debug": "debug", "release": "release"}

KnownGood = namedtuple("KnownGood", "path version")


def build_command(profile: str) -> list[str]:
    """Команда сборки для профиля (единый источник вместе с fresh_binary)."""
    if profile not in PROFILE_DIRS:
        raise ValueError(f"неизвестный build_profile: {profile}")
    return ["cargo", "build"] + (["--release"] if profile == "release" else [])


def fresh_binary(worktree: Path, profile: str) -> Path:
    """Путь к свежесобранному бинарю (только для observer)."""
    return Path(worktree) / "target" / PROFILE_DIRS[profile] / "clave"


def sanitized_env(worktree: Path, base_env: Optional[Mapping[str, str]] = None) -> dict:
    """Окружение для дочерних процессов без каталогов, где мог бы оказаться fresh clave:
    из PATH выкидываем target/debug, target/release и корень worktree ('.')."""
    env = dict(base_env if base_env is not None else os.environ)
    worktree = Path(worktree).resolve()
    forbidden = {
        str(worktree),
        str(worktree / "target" / "debug"),
        str(worktree / "target" / "release"),
    }
    parts = [p for p in env.get("PATH", "").split(os.pathsep) if p and str(Path(p).resolve()) not in forbidden]
    env["PATH"] = os.pathsep.join(parts)
    return env


def snapshot_known_good(known_good: Path, tmp_dir: Path) -> KnownGood:
    """Копируем known-good в приватный temp (чтобы посторонний cargo install не подменил)
    и логируем идентификацию версии."""
    known_good = Path(known_good).resolve()
    if not known_good.is_file():
        raise FileNotFoundError(f"known-good clave не найден: {known_good}")
    dest_dir = Path(tmp_dir) / "known-good"
    dest_dir.mkdir(parents=True, exist_ok=True)
    dest = dest_dir / "clave"
    shutil.copy2(known_good, dest)
    dest.chmod(0o755)
    try:
        version = subprocess.run(
            [str(dest), "--help"], capture_output=True, text=True, timeout=10
        ).stdout.splitlines()[0].strip()
    except Exception:
        version = "unknown"
    return KnownGood(path=dest, version=version)
```

`tools/clave-dev/tests/test_binaries.py`:
```python
import os
import unittest
from pathlib import Path

from clave_dev.binaries import build_command, fresh_binary, sanitized_env


class BinariesTest(unittest.TestCase):
    def test_build_command_and_fresh_path_share_profile(self):
        self.assertEqual(build_command("debug"), ["cargo", "build"])
        self.assertEqual(build_command("release"), ["cargo", "build", "--release"])
        self.assertEqual(
            fresh_binary(Path("/wt"), "debug"), Path("/wt/target/debug/clave")
        )
        self.assertEqual(
            fresh_binary(Path("/wt"), "release"), Path("/wt/target/release/clave")
        )

    def test_build_command_rejects_unknown_profile(self):
        with self.assertRaises(ValueError):
            build_command("nope")

    def test_sanitized_env_drops_target_and_repo_from_path(self):
        wt = Path("/tmp/wt").resolve()
        base = {
            "PATH": os.pathsep.join(
                [
                    "/usr/bin",
                    str(wt),
                    str(wt / "target" / "debug"),
                    str(wt / "target" / "release"),
                    "/usr/local/bin",
                ]
            )
        }
        env = sanitized_env(wt, base)
        parts = env["PATH"].split(os.pathsep)
        self.assertIn("/usr/bin", parts)
        self.assertIn("/usr/local/bin", parts)
        self.assertNotIn(str(wt), parts)
        self.assertNotIn(str(wt / "target" / "debug"), parts)
        self.assertNotIn(str(wt / "target" / "release"), parts)
```

- [ ] **Step 2: Запустить тесты (должны падать без модуля, проходить с ним)**

Run: `cd tools/clave-dev && python3 -m unittest tests.test_binaries -v`
Expected: 3 теста PASS.

- [ ] **Step 3: Commit**

```bash
git add tools/clave-dev/clave_dev/__init__.py tools/clave-dev/clave_dev/binaries.py tools/clave-dev/tests/__init__.py tools/clave-dev/tests/test_binaries.py
git commit -m "clave-dev: binary and PATH isolation module"
```

---

### Task 2: Git preflight и worktree (`worktree.py`)

**Files:**
- Create: `tools/clave-dev/clave_dev/worktree.py`
- Create: `tools/clave-dev/tests/test_worktree.py`

**Interfaces:**
- Produces: `class DirtyTreeError(RuntimeError)`; `assert_clean(repo: Path) -> None`; `create_run_worktree(repo: Path, base_ref: str, tmp_dir: Path) -> Path`; `remove_run_worktree(repo: Path, worktree: Path) -> None`.

- [ ] **Step 1: Написать `worktree.py` и падающие тесты**

`tools/clave-dev/clave_dev/worktree.py`:
```python
"""Git-безопасность: preflight чистого дерева + изолированный worktree на весь прогон."""
from __future__ import annotations

import subprocess
from pathlib import Path


class DirtyTreeError(RuntimeError):
    pass


def _git(repo: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", "-C", str(repo), *args], capture_output=True, text=True, check=False
    )


def assert_clean(repo: Path) -> None:
    """v1 не поддерживает dirty-запуск: грязное дерево → abort, ничего не трогаем."""
    out = _git(repo, "status", "--porcelain").stdout
    if out.strip():
        raise DirtyTreeError(
            "рабочее дерево не чистое; v1 требует чистый чекаут (закоммить/спрячь правки)"
        )


def create_run_worktree(repo: Path, base_ref: str, tmp_dir: Path) -> Path:
    """Создаёт изолированный worktree на detached HEAD от base_ref."""
    path = Path(tmp_dir) / "wt"
    res = _git(repo, "worktree", "add", "--detach", str(path), base_ref)
    if res.returncode != 0:
        raise RuntimeError(f"git worktree add не удался: {res.stderr.strip()}")
    return path


def remove_run_worktree(repo: Path, worktree: Path) -> None:
    _git(repo, "worktree", "remove", "--force", str(worktree))
    _git(repo, "worktree", "prune")
```

`tools/clave-dev/tests/test_worktree.py`:
```python
import subprocess
import tempfile
import unittest
from pathlib import Path

from clave_dev.worktree import DirtyTreeError, assert_clean, create_run_worktree, remove_run_worktree


def _init_repo(path: Path) -> None:
    for args in (
        ["init", "-q"],
        ["config", "user.email", "t@t"],
        ["config", "user.name", "t"],
    ):
        subprocess.run(["git", "-C", str(path), *args], check=True)
    (path / "f.txt").write_text("hi\n")
    subprocess.run(["git", "-C", str(path), "add", "."], check=True)
    subprocess.run(["git", "-C", str(path), "commit", "-q", "-m", "init"], check=True)


class WorktreeTest(unittest.TestCase):
    def test_assert_clean_passes_on_clean_and_raises_on_dirty(self):
        with tempfile.TemporaryDirectory() as d:
            repo = Path(d)
            _init_repo(repo)
            assert_clean(repo)  # чисто — не бросает
            (repo / "f.txt").write_text("changed\n")
            with self.assertRaises(DirtyTreeError):
                assert_clean(repo)

    def test_create_and_remove_worktree(self):
        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as t:
            repo = Path(d)
            _init_repo(repo)
            wt = create_run_worktree(repo, "HEAD", Path(t))
            self.assertTrue((wt / "f.txt").is_file())
            remove_run_worktree(repo, wt)
            self.assertFalse(wt.exists())
```

- [ ] **Step 2: Запустить тесты**

Run: `cd tools/clave-dev && python3 -m unittest tests.test_worktree -v`
Expected: 2 теста PASS (нужен установленный `git`).

- [ ] **Step 3: Commit**

```bash
git add tools/clave-dev/clave_dev/worktree.py tools/clave-dev/tests/test_worktree.py
git commit -m "clave-dev: git preflight and isolated worktree"
```

---

### Task 3: Cargo-проверки (`checks.py`)

**Files:**
- Create: `tools/clave-dev/clave_dev/checks.py`
- Create: `tools/clave-dev/tests/test_checks.py`

**Interfaces:**
- Consumes: `build_command` (Task 1).
- Produces: `ChecksResult = namedtuple("ChecksResult", "build_ok test_failures clippy_ok fmt_ok raw")`; `parse_test_failures(output: str) -> int`; `run_checks(worktree: Path, env: dict, profile: str) -> ChecksResult`.

- [ ] **Step 1: Написать `checks.py` и падающие тесты**

`tools/clave-dev/clave_dev/checks.py`:
```python
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


def _run(worktree: Path, env: dict, args: list[str]) -> subprocess.CompletedProcess:
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
    test_failures = parse_test_failures(raw["test"]) + (
        0 if test.returncode == 0 else max(1, 0)
    )

    clippy = _run(worktree, env, ["cargo", "clippy", "--all-targets", "--", "-D", "warnings"])
    raw["clippy"] = clippy.stdout + clippy.stderr
    clippy_ok = clippy.returncode == 0

    fmt = _run(worktree, env, ["cargo", "fmt", "--check"])
    raw["fmt"] = fmt.stdout + fmt.stderr
    fmt_ok = fmt.returncode == 0

    return ChecksResult(True, test_failures, clippy_ok, fmt_ok, raw)
```

`tools/clave-dev/tests/test_checks.py`:
```python
import unittest

from clave_dev.checks import parse_test_failures


class ChecksParseTest(unittest.TestCase):
    def test_zero_failures(self):
        out = "test result: ok. 107 passed; 0 failed; 0 ignored; 0 measured"
        self.assertEqual(parse_test_failures(out), 0)

    def test_counts_failures_across_result_lines(self):
        out = (
            "test result: FAILED. 5 passed; 2 failed; 0 ignored\n"
            "test result: FAILED. 3 passed; 1 failed; 0 ignored\n"
        )
        self.assertEqual(parse_test_failures(out), 3)

    def test_no_result_line_is_zero(self):
        self.assertEqual(parse_test_failures("compiler error, no tests ran"), 0)
```

- [ ] **Step 2: Запустить тесты**

Run: `cd tools/clave-dev && python3 -m unittest tests.test_checks -v`
Expected: 3 теста PASS.

- [ ] **Step 3: Commit**

```bash
git add tools/clave-dev/clave_dev/checks.py tools/clave-dev/tests/test_checks.py
git commit -m "clave-dev: cargo checks runner and parser"
```

---

### Task 4: Запуск агента и разбор `CLAVE-RUN` (`agent.py`)

**Files:**
- Create: `tools/clave-dev/clave_dev/agent.py`
- Create: `tools/clave-dev/tests/test_agent.py`

**Interfaces:**
- Produces: `AgentResult = namedtuple("AgentResult", "status code provider usage exit_code raw")`; `parse_clave_run(stdout: str, exit_code: int) -> AgentResult`; `run_agent(known_good: Path, worktree: Path, task: str, env: dict, effort: str|None, rounds: int|None) -> AgentResult`.

- [ ] **Step 1: Написать `agent.py` и падающие тесты**

`tools/clave-dev/clave_dev/agent.py`:
```python
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
```

`tools/clave-dev/tests/test_agent.py`:
```python
import unittest

from clave_dev.agent import parse_clave_run


class AgentParseTest(unittest.TestCase):
    def test_parses_completed_line(self):
        out = (
            "activity...\n"
            'CLAVE-RUN {"status":"completed","code":0,"provider":"codex",'
            '"usage":{"input":60,"output":30},"ended_reason":"completed"}\n'
        )
        r = parse_clave_run(out, 0)
        self.assertEqual(r.status, "completed")
        self.assertEqual(r.code, 0)
        self.assertEqual(r.provider, "codex")
        self.assertEqual(r.usage["input"], 60)
        self.assertEqual(r.exit_code, 0)

    def test_no_marker(self):
        r = parse_clave_run("just some output", 3)
        self.assertEqual(r.status, "no_marker")
        self.assertEqual(r.exit_code, 3)
```

- [ ] **Step 2: Запустить тесты**

Run: `cd tools/clave-dev && python3 -m unittest tests.test_agent -v`
Expected: 2 теста PASS.

- [ ] **Step 3: Commit**

```bash
git add tools/clave-dev/clave_dev/agent.py tools/clave-dev/tests/test_agent.py
git commit -m "clave-dev: headless agent runner and CLAVE-RUN parser"
```

---

### Task 5: Observer и assertions (`assertions.py`, `observer.py`)

**Files:**
- Create: `tools/clave-dev/clave_dev/assertions.py`
- Create: `tools/clave-dev/clave_dev/observer.py`
- Create: `tools/clave-dev/tests/test_assertions.py`

**Interfaces:**
- Produces: `AssertionResult = namedtuple("AssertionResult", "name passed message")`; фабрики `visible(sub)`, `not_visible(sub)`, `line_matches(pattern)`, `no_line_overflows_width(width)` — каждая возвращает `callable(grid: list[str], exit_code: int) -> AssertionResult`; `structural_assertions() -> list[callable]`; `evaluate(assertions, grid, exit_code) -> list[AssertionResult]`.
- Observer: `Scenario = namedtuple("Scenario", "name steps settle_s assertions")`; `run_scenario(binary: Path, env: dict, scenario: Scenario, cols=100, rows=30) -> tuple[list[str], list[AssertionResult]]`.

- [ ] **Step 1: Написать `assertions.py` + тесты (чистая логика)**

`tools/clave-dev/clave_dev/assertions.py`:
```python
"""Машинно-проверяемые предикаты над символьной сеткой (список строк)."""
from __future__ import annotations

import re
from collections import namedtuple

AssertionResult = namedtuple("AssertionResult", "name passed message")


def visible(sub: str):
    def check(grid, exit_code):
        ok = any(sub in row for row in grid)
        return AssertionResult(f"visible({sub!r})", ok, "" if ok else f"не найдено: {sub!r}")
    return check


def not_visible(sub: str):
    def check(grid, exit_code):
        ok = all(sub not in row for row in grid)
        return AssertionResult(f"not_visible({sub!r})", ok, "" if ok else f"найдено, но не должно: {sub!r}")
    return check


def line_matches(pattern: str):
    rx = re.compile(pattern)
    def check(grid, exit_code):
        ok = any(rx.search(row) for row in grid)
        return AssertionResult(f"line_matches({pattern!r})", ok, "" if ok else "нет строки под шаблон")
    return check


def no_line_overflows_width(width: int):
    def check(grid, exit_code):
        bad = next((r for r in grid if len(r.rstrip()) > width), None)
        ok = bad is None
        return AssertionResult("no_line_overflows_width", ok, "" if ok else f"строка шире {width}: {bad!r}")
    return check


def launched():
    def check(grid, exit_code):
        ok = any(row.strip() for row in grid)
        return AssertionResult("launched", ok, "" if ok else "экран пуст")
    return check


def clean_exit():
    def check(grid, exit_code):
        ok = exit_code == 0
        return AssertionResult("clean_exit", ok, "" if ok else f"exit={exit_code}")
    return check


def structural_assertions():
    """Базовые assertions, активные всегда (спека §5)."""
    return [launched(), clean_exit()]


def evaluate(assertions, grid, exit_code):
    return [a(grid, exit_code) for a in assertions]
```

`tools/clave-dev/tests/test_assertions.py`:
```python
import unittest

from clave_dev.assertions import (
    clean_exit,
    evaluate,
    line_matches,
    no_line_overflows_width,
    not_visible,
    visible,
)


class AssertionsTest(unittest.TestCase):
    def setUp(self):
        self.grid = ["  Отправка", "  Enter  отправить", "  Ctrl+R поиск"]

    def test_visible_and_not_visible(self):
        self.assertTrue(visible("Отправка")(self.grid, 0).passed)
        self.assertFalse(visible("Управление")(self.grid, 0).passed)
        self.assertTrue(not_visible("Управление")(self.grid, 0).passed)
        self.assertFalse(not_visible("Отправка")(self.grid, 0).passed)

    def test_line_matches_and_overflow_and_exit(self):
        self.assertTrue(line_matches(r"Enter\s+отправить")(self.grid, 0).passed)
        self.assertTrue(no_line_overflows_width(40)(self.grid, 0).passed)
        self.assertFalse(no_line_overflows_width(5)(self.grid, 0).passed)
        self.assertFalse(clean_exit()(self.grid, 1).passed)

    def test_evaluate_returns_result_per_assertion(self):
        results = evaluate([visible("Отправка"), not_visible("X")], self.grid, 0)
        self.assertEqual(len(results), 2)
        self.assertTrue(all(r.passed for r in results))
```

- [ ] **Step 2: Написать `observer.py` (pty-драйв, как в `scripts/render_check.py`)**

`tools/clave-dev/clave_dev/observer.py`:
```python
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

import pyte

from .assertions import evaluate

Scenario = namedtuple("Scenario", "name steps settle_s assertions")


def run_scenario(binary: Path, env: dict, scenario: Scenario, cols: int = 100, rows: int = 30):
    master, slave = pty.openpty()
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
    run_env = dict(env)
    run_env.setdefault("TERM", "xterm-256color")
    run_env.setdefault("CLAVE_SKIP_ONBOARDING", "1")
    proc = subprocess.Popen(
        [str(binary)], stdin=slave, stdout=slave, stderr=slave, env=run_env, close_fds=True
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

    os.write(master, b"/quit\r")
    pump(0.6)
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
```

- [ ] **Step 3: Запустить тесты assertions**

Run: `cd tools/clave-dev && python3 -m unittest tests.test_assertions -v`
Expected: 3 теста PASS. (Observer покрывается end-to-end смоуком в Task 7; юнит-тест pty здесь избыточен.)

- [ ] **Step 4: Commit**

```bash
git add tools/clave-dev/clave_dev/assertions.py tools/clave-dev/clave_dev/observer.py tools/clave-dev/tests/test_assertions.py
git commit -m "clave-dev: observer scenarios and machine-checkable assertions"
```

---

### Task 6: Петля, критерий останова, контекст, отчёт (`context.py`, `loop.py`, `report.py`)

**Files:**
- Create: `tools/clave-dev/clave_dev/context.py`
- Create: `tools/clave-dev/clave_dev/loop.py`
- Create: `tools/clave-dev/clave_dev/report.py`
- Create: `tools/clave-dev/tests/test_loop.py`

**Interfaces:**
- Consumes: `ChecksResult` (Task 3), `AssertionResult` (Task 5), `AgentResult` (Task 4), observer `run_scenario`, `run_agent`, `run_checks`.
- Produces: `converged(checks, assertion_results) -> bool`; `build_context(checks, grids, assertion_results) -> str`; `RunConfig`/`RunReport` (namedtuples); `run_loop(cfg) -> RunReport`; `render_report(report, repo, worktree) -> str`.

- [ ] **Step 1: Написать `context.py` + `report.py` + `loop.py`**

`tools/clave-dev/clave_dev/context.py`:
```python
"""Сборка текстового контекст-блока для следующего раунда агента (спека §3)."""
from __future__ import annotations


def build_context(checks, grids, assertion_results) -> str:
    lines = ["## Проверки"]
    if checks is None:
        lines.append("- (ещё не запускались)")
    else:
        lines.append(f"- build: {'ok' if checks.build_ok else 'FAIL'}")
        lines.append(f"- test failures: {checks.test_failures}")
        lines.append(f"- clippy: {'ok' if checks.clippy_ok else 'FAIL (-D warnings)'}")
        lines.append(f"- fmt: {'ok' if checks.fmt_ok else 'FAIL'}")
        for name in ("build", "test", "clippy", "fmt"):
            chunk = (checks.raw or {}).get(name, "")
            tail = "\n".join(chunk.splitlines()[-20:])
            if tail.strip():
                lines.append(f"\n### вывод {name} (хвост)\n{tail}")
    lines.append("\n## Экран")
    for i, grid in enumerate(grids):
        lines.append(f"\n### сценарий {i}\n" + "\n".join(grid))
    lines.append("\n## Assertions")
    for r in assertion_results:
        lines.append(f"- {'PASS' if r.passed else 'FAIL'} {r.name} {r.message}")
    return "\n".join(lines)
```

`tools/clave-dev/clave_dev/report.py`:
```python
"""Финальный отчёт: диф, версии, проверки, assertions (спека §7). Без коммита/установки."""
from __future__ import annotations

import subprocess
from pathlib import Path


def render_report(report, repo: Path, worktree: Path) -> str:
    diff = subprocess.run(
        ["git", "-C", str(worktree), "diff"], capture_output=True, text=True
    ).stdout
    lines = [
        "# clave-dev: итог прогона (стоп перед финалом)",
        f"known-good: {report.known_good_version}",
        f"раундов: {report.rounds_used} / лимит {report.max_rounds}",
        f"сошлось: {'да' if report.converged else 'нет'}",
        "",
        "## Assertions (последний раунд)",
    ]
    for r in report.last_assertions:
        lines.append(f"- {'PASS' if r.passed else 'FAIL'} {r.name} {r.message}")
    lines += ["", "## Diff", diff if diff.strip() else "(нет изменений)"]
    lines += ["", f"worktree: {worktree}", "Ни коммита, ни установки не сделано — ревьюь и решай."]
    return "\n".join(lines)
```

`tools/clave-dev/clave_dev/loop.py`:
```python
"""Оркестрация: implement → checks → observe → judge, до критерия или лимита раундов."""
from __future__ import annotations

from collections import namedtuple

from .agent import run_agent
from .assertions import structural_assertions
from .checks import run_checks
from .context import build_context
from .observer import run_scenario

RunConfig = namedtuple(
    "RunConfig",
    "known_good worktree repo env profile task effort rounds max_rounds scenarios",
)
RunReport = namedtuple(
    "RunReport",
    "converged rounds_used max_rounds last_assertions known_good_version",
)


def converged(checks, assertion_results) -> bool:
    """Спека §5: build ок И test 0 И clippy ок И fmt ок И все assertions pass."""
    if checks is None or not checks.build_ok:
        return False
    checks_ok = checks.test_failures == 0 and checks.clippy_ok and checks.fmt_ok
    asserts_ok = all(r.passed for r in assertion_results)
    return checks_ok and asserts_ok


def run_loop(cfg: RunConfig, known_good_version: str) -> RunReport:
    checks = None
    grids = []
    assertion_results = []
    context = ""
    for round_i in range(1, cfg.max_rounds + 1):
        task = cfg.task if not context else f"{cfg.task}\n\n{context}"
        run_agent(cfg.known_good, cfg.worktree, task, cfg.env, cfg.effort, cfg.rounds)

        checks = run_checks(cfg.worktree, cfg.env, cfg.profile)
        grids, assertion_results = [], []
        if checks.build_ok:
            from .binaries import fresh_binary

            fresh = fresh_binary(cfg.worktree, cfg.profile)
            for scenario in cfg.scenarios:
                scenario = scenario._replace(
                    assertions=list(structural_assertions()) + list(scenario.assertions)
                )
                grid, results = run_scenario(fresh, cfg.env, scenario)
                grids.append(grid)
                assertion_results.extend(results)

        if converged(checks, assertion_results):
            return RunReport(True, round_i, cfg.max_rounds, assertion_results, known_good_version)
        context = build_context(checks, grids, assertion_results)

    return RunReport(False, cfg.max_rounds, cfg.max_rounds, assertion_results, known_good_version)
```

`tools/clave-dev/tests/test_loop.py`:
```python
import unittest

from clave_dev.assertions import AssertionResult
from clave_dev.checks import ChecksResult
from clave_dev.loop import converged


class ConvergedTest(unittest.TestCase):
    def _checks(self, **kw):
        base = dict(build_ok=True, test_failures=0, clippy_ok=True, fmt_ok=True, raw={})
        base.update(kw)
        return ChecksResult(**base)

    def test_all_green_and_assertions_pass_converges(self):
        asserts = [AssertionResult("a", True, ""), AssertionResult("b", True, "")]
        self.assertTrue(converged(self._checks(), asserts))

    def test_failing_check_blocks(self):
        self.assertFalse(converged(self._checks(clippy_ok=False), []))
        self.assertFalse(converged(self._checks(test_failures=2), []))
        self.assertFalse(converged(self._checks(build_ok=False), []))

    def test_failing_assertion_blocks(self):
        asserts = [AssertionResult("a", True, ""), AssertionResult("b", False, "nope")]
        self.assertFalse(converged(self._checks(), asserts))
```

- [ ] **Step 2: Запустить тесты**

Run: `cd tools/clave-dev && python3 -m unittest tests.test_loop -v`
Expected: 3 теста PASS.

- [ ] **Step 3: Commit**

```bash
git add tools/clave-dev/clave_dev/context.py tools/clave-dev/clave_dev/loop.py tools/clave-dev/clave_dev/report.py tools/clave-dev/tests/test_loop.py
git commit -m "clave-dev: loop, convergence, context and report"
```

---

### Task 7: CLI + end-to-end смоук на моках

**Files:**
- Create: `tools/clave-dev/clave_dev/cli.py`
- Create: `tools/clave-dev/clave_dev/__main__.py`
- Create: `tools/clave-dev/scripts/smoke_loop.sh`

**Interfaces:**
- Consumes: все модули Tasks 1–6.
- Produces: `main(argv=None) -> int`.

- [ ] **Step 1: Написать `cli.py` и `__main__.py`**

`tools/clave-dev/clave_dev/cli.py`:
```python
"""CLI супервайзера: собирает изоляцию, worktree, сценарии и гоняет петлю."""
from __future__ import annotations

import argparse
import os
import sys
import tempfile
from pathlib import Path

from .assertions import line_matches, not_visible, visible
from .binaries import sanitized_env, snapshot_known_good
from .loop import RunConfig, run_loop
from .observer import Scenario
from .report import render_report
from .worktree import assert_clean, create_run_worktree, remove_run_worktree

_ASSERT_FACTORIES = {"visible": visible, "not_visible": not_visible, "line_matches": line_matches}


def _parse_assert(spec: str):
    kind, _, arg = spec.partition(":")
    if kind not in _ASSERT_FACTORIES:
        raise argparse.ArgumentTypeError(f"неизвестный assert: {kind}")
    return _ASSERT_FACTORIES[kind](arg)


def main(argv=None) -> int:
    p = argparse.ArgumentParser(prog="clave-dev")
    p.add_argument("task")
    p.add_argument("--repo", default=".", type=Path)
    p.add_argument("--known-good", default=str(Path.home() / ".cargo/bin/clave"), type=Path)
    p.add_argument("--build-profile", default="debug", choices=["debug", "release"])
    p.add_argument("--rounds", type=int, default=None, help="debate-раунды tandem")
    p.add_argument("--max-rounds", type=int, default=3, help="раундов петли супервайзера")
    p.add_argument("--effort", default=None)
    p.add_argument("--assert", dest="asserts", action="append", type=_parse_assert, default=[])
    args = p.parse_args(argv)

    repo = args.repo.resolve()
    assert_clean(repo)
    tmp = Path(tempfile.mkdtemp(prefix="clave-dev-"))
    worktree = create_run_worktree(repo, "HEAD", tmp)
    try:
        known = snapshot_known_good(args.known_good, tmp)
        env = sanitized_env(worktree)
        scenario = Scenario(name="default", steps=[("?", 0.4)], settle_s=0.3, assertions=args.asserts)
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
        return 0 if report.converged else 1
    finally:
        # worktree и его diff остаются для ревью; чистим только если пусто? — оставляем.
        # (Отбрасывание/уборка — отдельным решением человека, спека §7.)
        pass
```

`tools/clave-dev/clave_dev/__main__.py`:
```python
import sys

from .cli import main

if __name__ == "__main__":
    sys.exit(main())
```

> Примечание: worktree сознательно НЕ удаляется в `finally` — его diff нужен человеку для ревью (спека §7). Удаление/отбрасывание — отдельный человеческий шаг (`git worktree remove`). Это допустимо для v1; при желании добавить `--cleanup` позже.

- [ ] **Step 2: Смоук петли на мок-провайдерах**

`tools/clave-dev/scripts/smoke_loop.sh`:
```bash
#!/bin/bash
# End-to-end смоук супервайзера на моках: тривиальная задача, реальный headless clave
# (known-good) с мок-провайдерами, свежий build в observer. Проверяем, что петля
# отрабатывает раунд, собирает отчёт и завершается (сходимость не обязательна на моках).
set -u
root="$(cd "$(dirname "$0")/../../.." && pwd)"        # корень репо clave
selfdev="$root/scripts/selfdev"
kg="$1"                                                # known-good clave (напр. ~/.cargo/bin/clave)
venv="${2:?путь к python с pyte}"                      # напр. .../venv/bin/python3
export CLAVE_CLAUDE="$selfdev/mock-claude.sh" CLAVE_CODEX="$selfdev/mock-codex.sh"
cd "$root/tools/clave-dev"
"$venv" -m clave_dev "поменяй подпись в футере" \
  --repo "$root" --known-good "$kg" --build-profile debug --max-rounds 1 \
  && echo "SMOKE: loop ran and produced a report"
```

- [ ] **Step 3: Прогнать все юнит-тесты и смоук**

Run:
```bash
cd tools/clave-dev && python3 -m unittest discover -s tests -v
# смоук (нужен venv с pyte из Plan 1 и чистое дерево репо):
chmod +x scripts/smoke_loop.sh
scripts/smoke_loop.sh "$HOME/.cargo/bin/clave" "<путь>/venv/bin/python3"
```
Expected: все юнит-тесты (binaries/worktree/checks/agent/assertions/loop) PASS; смоук печатает отчёт `clave-dev: итог прогона …` и `SMOKE: loop ran and produced a report`.

- [ ] **Step 4: Commit**

```bash
git add tools/clave-dev/clave_dev/cli.py tools/clave-dev/clave_dev/__main__.py tools/clave-dev/scripts/smoke_loop.sh
git commit -m "clave-dev: CLI wiring and end-to-end loop smoke"
```

---

## Self-Review

- **Покрытие спеки:** §2 компоненты — Tasks 1–7; §3 контракт потребляется `agent.py` (Task 4, парсинг CLAVE-RUN, feedback через stdin в `loop`); §4 петля/build_profile — `loop.py`+`binaries.py` (Task 6/1); §5 observer+assertions+критерий — Tasks 5–6 (`converged`); §6 изоляция бинарей/PATH — Task 1 (`sanitized_env`, `snapshot_known_good`) + инвариант-тест; §7 git/worktree+отчёт — Tasks 2, 6 (`report`), финал без коммита/установки; §8 ошибки/разгон — `max_rounds` в `loop`, preflight в `worktree`; §9 тесты — юниты в каждом таске + смоук (Task 7).
- **Плейсхолдеры:** нет; код приведён по модулям, тесты с реальными ассертами.
- **Согласованность типов:** `ChecksResult`/`AgentResult`/`AssertionResult`/`Scenario`/`RunConfig`/`RunReport` — namedtuple с фиксированными полями, используются согласованно между `checks`→`loop`→`report`, `agent`→`loop`, `assertions`→`observer`→`loop`. `build_command`/`fresh_binary` делят `PROFILE_DIRS`. `run_scenario` возвращает `(grid, results)`, как ждёт `loop`.
- **Замечания для исполнителя:** (1) `checks.run_checks` для `test_failures` опирается на строки `test result:`; если билд падает — возвращаем рано. (2) observer требует `pyte` (venv из Plan 1). (3) смоук предполагает чистое дерево репо (preflight) — запускать из чистого состояния (напр. эта ветка без незакоммиченного). (4) worktree в v1 не удаляется автоматически (нужен diff для ревью) — человек убирает `git worktree remove`.

## Границы Plan 2

Даёт рабочую петлю v1 (Фаза 1). НЕ входит: vision по реальному терминалу (Фаза 2), интеграция в TUI (Фаза 3), авто-финализация, работа над чужим repo, параллельные кандидаты — отдельными циклами поверх готового v1.
