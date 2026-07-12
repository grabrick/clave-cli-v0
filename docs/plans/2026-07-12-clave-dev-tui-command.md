# clave-dev — Фаза 3 (`/dev` в TUI): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`).

**Goal:** Команда `/dev <задача>` в TUI запускает внешний `clave-dev` на текущем репозитории, стримит типизированный прогресс в транскрипт, показывает финальный отчёт, останавливается на ревью — не нарушая инвариант «control plane вне пересобираемого бинаря».

**Architecture:** Две стороны. Python: типизированный протокол `CLAVE-DEV <type> <payload>`, diff-билдер, идентичность known-good через `--version`+sha256, git-root. Rust (TUI): `/dev` по образцу `start_task` (`src/app/runs.rs`) — `spawn_worker` + `Command` + `configure_process_group` + читатель stdout с парсингом типов + буфер stderr + цикл отмены; busy-preflight через `self.running`; парсинг строк — чистой функцией.

**Tech Stack:** Python 3.9 stdlib (+ Фазы 1/2). Rust 2021 (crossterm/ratatui), переиспуём `worker.rs`: `spawn_worker`, `spawn_reader`, `configure_process_group`, `kill_process_tree`, `WorkerEvent`. Spec: `docs/design/2026-07-12-clave-dev-tui-command.md`.

## Global Constraints

- Инвариант: супервайзер — внешний процесс; TUI даёт триггер+вид, петля НЕ внутри clave.
- known-good = **абсолютный** `env::current_exe()` (не PATH-имя); супервайзер копирует в temp + логирует `--version`(+фолбэк `--help`)+sha256 (§3 спеки).
- Поиск `clave_dev`: `CLAVE_DEV_HOME` → `<git root>/tools/clave-dev` → установленный модуль → внятная ошибка (§4).
- repo канонизируется до git-корня (`git rev-parse --show-toplevel`) до поиска/спавна (§4).
- protocol-mode: stdout — только обрамлённые `CLAVE-DEV …` строки; сырьё под-процессов → `log`/stderr; raw tail stderr при сбое (§5).
- busy-preflight: идёт agent/dev-прогон (`self.running`) → `/dev` отвечает «занят», второй процесс не стартует (§6).
- Отмена — `kill_process_tree` по группе (§6). Финал — стоп на ревью, без коммита/установки.
- Python 3.9: `from __future__ import annotations`. Rust: mirror существующих паттернов `runs.rs`/`worker.rs`.

---

### Task 1: Типизированный эмиттер протокола (`emit.py`)

**Files:**
- Create: `tools/clave-dev/clave_dev/emit.py`
- Create: `tools/clave-dev/tests/test_emit.py`

**Interfaces:**
- Produces: `EMIT_TYPES`; `format_line(type_: str, payload) -> str`; `class Emitter` c `__init__(self, enabled, out=None)` и методами `progress/log/check/vision/diff/report/error`; `no_op_emitter()`.

- [ ] **Step 1: Написать модуль и падающие тесты**

`tools/clave-dev/clave_dev/emit.py`:
```python
"""Типизированный протокол прогресса CLAVE-DEV <type> <payload> для TUI (спека §5)."""
from __future__ import annotations

import json
import sys

EMIT_TYPES = ("progress", "log", "check", "vision", "diff", "report", "error")


def format_line(type_: str, payload) -> str:
    """Одна обрамлённая строка. Текст для progress/log/error, JSON для check/vision/diff/report."""
    if type_ not in EMIT_TYPES:
        raise ValueError(f"неизвестный тип события: {type_}")
    if type_ in ("progress", "log", "error"):
        body = payload if isinstance(payload, str) else json.dumps(payload, ensure_ascii=False)
    else:
        body = json.dumps(payload, ensure_ascii=False)
    return f"CLAVE-DEV {type_} {body}"


class Emitter:
    """enabled=False → no-op (standalone-CLI Фазы 1/2 не засоряется). enabled=True →
    печатает обрамлённые строки в out (stdout по умолчанию)."""

    def __init__(self, enabled: bool, out=None):
        self.enabled = enabled
        self._out = out if out is not None else sys.stdout

    def emit(self, type_: str, payload) -> None:
        if not self.enabled:
            return
        print(format_line(type_, payload), file=self._out, flush=True)

    def progress(self, text): self.emit("progress", text)
    def log(self, text): self.emit("log", text)
    def check(self, payload): self.emit("check", payload)
    def vision(self, payload): self.emit("vision", payload)
    def diff(self, payload): self.emit("diff", payload)
    def report(self, payload): self.emit("report", payload)
    def error(self, text): self.emit("error", text)


def no_op_emitter() -> Emitter:
    return Emitter(enabled=False)
```

`tools/clave-dev/tests/test_emit.py`:
```python
import io
import unittest

from clave_dev.emit import Emitter, format_line


class EmitTest(unittest.TestCase):
    def test_format_text_and_json_types(self):
        self.assertEqual(format_line("progress", "раунд 1"), "CLAVE-DEV progress раунд 1")
        line = format_line("check", {"name": "build", "ok": True})
        self.assertTrue(line.startswith("CLAVE-DEV check "))
        self.assertIn('"name": "build"', line)

    def test_unknown_type_raises(self):
        with self.assertRaises(ValueError):
            format_line("nope", "x")

    def test_disabled_emitter_is_silent(self):
        buf = io.StringIO()
        Emitter(enabled=False, out=buf).progress("тихо")
        self.assertEqual(buf.getvalue(), "")

    def test_enabled_emitter_writes_framed_line(self):
        buf = io.StringIO()
        Emitter(enabled=True, out=buf).report({"converged": True, "rounds": 1})
        self.assertIn("CLAVE-DEV report ", buf.getvalue())
        self.assertIn('"converged": true', buf.getvalue())
```

- [ ] **Step 2: Прогнать** — `cd tools/clave-dev && python3 -m unittest tests.test_emit -v` → 4 PASS.
- [ ] **Step 3: Commit** — `git add … && git commit -m "clave-dev: typed CLAVE-DEV progress emitter"`

---

### Task 2: Diff-билдер (`diff.py`)

**Files:**
- Create: `tools/clave-dev/clave_dev/diff.py`
- Create: `tools/clave-dev/tests/test_diff.py`

**Interfaces:**
- Produces: `build_diff(worktree: Path, patch_path: Path, max_files: int = 200) -> dict` с ключами `stat, changed_files, patch_path, truncated`.

- [ ] **Step 1: Написать модуль и падающие тесты**

`tools/clave-dev/clave_dev/diff.py`:
```python
"""Сводка правок для показа дифа в TUI: stat + список файлов + путь к полному патчу (спека §5)."""
from __future__ import annotations

import subprocess
from pathlib import Path


def _git(worktree: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(worktree), *args], capture_output=True, text=True, check=False
    ).stdout


def build_diff(worktree: Path, patch_path: Path, max_files: int = 200) -> dict:
    """Полный патч пишется в patch_path (не льётся в транскрипт); в TUI идут stat+файлы."""
    stat = _git(worktree, "diff", "--stat").strip()
    files = [f for f in _git(worktree, "diff", "--name-only").splitlines() if f.strip()]
    patch = _git(worktree, "diff")
    Path(patch_path).write_text(patch)
    truncated = len(files) > max_files
    return {
        "stat": stat,
        "changed_files": files[:max_files],
        "patch_path": str(patch_path),
        "truncated": truncated,
    }
```

`tools/clave-dev/tests/test_diff.py`:
```python
import subprocess
import tempfile
import unittest
from pathlib import Path

from clave_dev.diff import build_diff


def _repo(path: Path):
    for a in (["init", "-q"], ["config", "user.email", "t@t"], ["config", "user.name", "t"]):
        subprocess.run(["git", "-C", str(path), *a], check=True)
    (path / "f.txt").write_text("one\n")
    subprocess.run(["git", "-C", str(path), "add", "."], check=True)
    subprocess.run(["git", "-C", str(path), "commit", "-q", "-m", "init"], check=True)


class DiffTest(unittest.TestCase):
    def test_build_diff_reports_changed_files_and_writes_patch(self):
        with tempfile.TemporaryDirectory() as d:
            wt = Path(d)
            _repo(wt)
            (wt / "f.txt").write_text("two\n")
            patch = wt / "patch.diff"
            out = build_diff(wt, patch)
            self.assertIn("f.txt", out["changed_files"])
            self.assertFalse(out["truncated"])
            self.assertTrue(patch.is_file() and "two" in patch.read_text())

    def test_clean_tree_is_empty(self):
        with tempfile.TemporaryDirectory() as d:
            wt = Path(d)
            _repo(wt)
            out = build_diff(wt, wt / "p.diff")
            self.assertEqual(out["changed_files"], [])
```

- [ ] **Step 2: Прогнать** → 2 PASS.
- [ ] **Step 3: Commit** — `"clave-dev: diff summary builder for TUI"`

---

### Task 3: Идентичность known-good — `--version` + sha256 (`binaries.py`)

**Files:**
- Modify: `tools/clave-dev/clave_dev/binaries.py`
- Modify: `tools/clave-dev/tests/test_binaries.py`

**Interfaces:**
- Consumes/Produces: `sha256_file(path) -> str`; `identify_binary(path) -> str` (`--version`, фолбэк первая строка `--help`); `KnownGood` получает поле `hash`; `snapshot_known_good` использует их.

- [ ] **Step 1: Тесты (сперва)** — добавить в `test_binaries.py`:
```python
import hashlib
import os
import stat
import tempfile
from pathlib import Path

from clave_dev.binaries import identify_binary, sha256_file


class IdentityTest(unittest.TestCase):
    def test_sha256_file(self):
        with tempfile.NamedTemporaryFile(delete=False) as f:
            f.write(b"clave-bin")
            p = f.name
        self.addCleanup(os.unlink, p)
        self.assertEqual(sha256_file(Path(p)), hashlib.sha256(b"clave-bin").hexdigest())

    def test_identify_prefers_version_over_help(self):
        with tempfile.TemporaryDirectory() as d:
            fake = Path(d) / "clave"
            fake.write_text('#!/bin/bash\n[ "$1" = "--version" ] && echo "clave 9.9.9" && exit 0\necho "help top"\n')
            fake.chmod(fake.stat().st_mode | stat.S_IEXEC)
            self.assertEqual(identify_binary(fake), "clave 9.9.9")

    def test_identify_falls_back_to_help(self):
        with tempfile.TemporaryDirectory() as d:
            fake = Path(d) / "clave"
            fake.write_text('#!/bin/bash\n[ "$1" = "--version" ] && exit 2\necho "help first line"\n')
            fake.chmod(fake.stat().st_mode | stat.S_IEXEC)
            self.assertEqual(identify_binary(fake), "help first line")
```

- [ ] **Step 2: Реализация** — в `binaries.py`:
```python
import hashlib

def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def identify_binary(path: Path) -> str:
    """Идентификация: `--version` (первая строка), фолбэк на первую строку `--help`."""
    for flag in ("--version", "--help"):
        try:
            res = subprocess.run([str(path), flag], capture_output=True, text=True, timeout=10)
            if res.returncode == 0 and res.stdout.strip():
                return res.stdout.splitlines()[0].strip()
        except Exception:
            continue
    return "unknown"
```
Заменить в `KnownGood` поля на `"path version hash"` и в `snapshot_known_good` вернуть `KnownGood(path=dest, version=identify_binary(dest), hash=sha256_file(dest))`.

> Правка существующих тестов: старые обращения к `KnownGood` (Task 1 Фазы 1) не распаковывают поля позиционно — совместимо. Если где-то создаётся `KnownGood(path=..., version=...)` в тестах — добавить `hash="…"`.

- [ ] **Step 3: Прогнать** `tests.test_binaries` → все PASS (старые + 3 новых).
- [ ] **Step 4: Commit** — `"clave-dev: known-good identity via --version and sha256"`

---

### Task 4: git-root канонизация (`worktree.py`)

**Files:**
- Modify: `tools/clave-dev/clave_dev/worktree.py`
- Modify: `tools/clave-dev/tests/test_worktree.py`

**Interfaces:** Produces `git_root(path: Path) -> Path` (через `git rev-parse --show-toplevel`).

- [ ] **Step 1: Тест** — добавить в `test_worktree.py`:
```python
from clave_dev.worktree import git_root

class GitRootTest(unittest.TestCase):
    def test_git_root_from_subdir(self):
        with tempfile.TemporaryDirectory() as d:
            repo = Path(d)
            _init_repo(repo)
            sub = repo / "tools" / "x"
            sub.mkdir(parents=True)
            self.assertEqual(git_root(sub).resolve(), repo.resolve())
```

- [ ] **Step 2: Реализация** — в `worktree.py`:
```python
def git_root(path: Path) -> Path:
    """Канонический git-корень для path (спека §4). Не git → RuntimeError."""
    res = subprocess.run(
        ["git", "-C", str(path), "rev-parse", "--show-toplevel"],
        capture_output=True, text=True, check=False,
    )
    if res.returncode != 0:
        raise RuntimeError(f"не git-репозиторий: {path}")
    return Path(res.stdout.strip())
```

- [ ] **Step 3: Прогнать** → PASS. **Commit** — `"clave-dev: git root canonicalization"`

---

### Task 5: Проводка эмиттера в петлю и CLI (`loop.py`, `cli.py`)

**Files:**
- Modify: `tools/clave-dev/clave_dev/loop.py` (run_loop принимает emitter, эмитит события)
- Modify: `tools/clave-dev/clave_dev/cli.py` (флаг `--protocol`, сборка emitter, diff в финале)
- Create: `tools/clave-dev/tests/test_loop_emit.py`

**Interfaces:**
- Consumes: `Emitter` (Task 1), `build_diff` (Task 2).
- Produces: `run_loop(cfg, known_good_version, emitter=None)` — эмитит `progress`/`check`/`vision`/`report`; cli при `--protocol clave-dev` строит `Emitter(True)` и по завершении — `diff`.

- [ ] **Step 1: Тест (сперва)** — `tools/clave-dev/tests/test_loop_emit.py`:
```python
import unittest

from clave_dev.emit import Emitter


class CapturingOut:
    def __init__(self): self.lines = []
    def write(self, s):
        if s.strip():
            self.lines.append(s.strip())
    def flush(self): pass


class LoopEmitTest(unittest.TestCase):
    def test_emitter_check_lines_are_framed(self):
        out = CapturingOut()
        em = Emitter(enabled=True, out=out)
        em.progress("round 1")
        em.check({"name": "build", "ok": True})
        self.assertTrue(any(l.startswith("CLAVE-DEV progress") for l in out.lines))
        self.assertTrue(any(l.startswith("CLAVE-DEV check") for l in out.lines))
```
> Примечание: `run_loop` целиком гоняется только на реальном cargo (см. мок-смоук Task 8 стороны Rust). Здесь юнитом фиксируем контракт эмиттера; проводку в `run_loop` проверяем тем, что эмиттер вызывается (ниже).

- [ ] **Step 2: Реализация** — в `loop.py` добавить параметр `emitter=None`, в начале `run_loop` `emitter = emitter or no_op_emitter()`; эмитить: `emitter.progress(f"раунд {round_i}")` перед агентом; после `run_checks` — `emitter.check({"name": n, "ok": ...})` по каждой проверке; при наличии vision — `emitter.vision({"pass": all(...), "issues": N})`; перед возвратом — `emitter.report({"converged": ..., "rounds": round_i, "max_rounds": cfg.max_rounds, "worktree": str(cfg.worktree), "known_good": known_good_version})`. В `cli.py`: `--protocol` choice `["clave-dev"]` default None; `emitter = Emitter(enabled=(args.protocol == "clave-dev"))`; передать в `run_loop`; после — `emitter.diff(build_diff(worktree, worktree/".clave-dev.patch"))`.

- [ ] **Step 3: Прогнать весь набор** — `python3 -m unittest discover -s tests` → всё PASS (Фазы 1/2/3, ~60 тестов).
- [ ] **Step 4: Commit** — `"clave-dev: wire emitter and diff into loop and CLI"`

---

### Task 6: `clave --version` (Rust, `runtime.rs`)

**Files:**
- Modify: `src/runtime.rs` (обработка `--version`)

**Interfaces:** `clave --version` печатает `clave <ver>` и выходит 0 (используется `identify_binary`, §3).

- [ ] **Step 1: Найти ветку разбора флагов** — в `src/runtime.rs` рядом с обработкой `-h/--help` и `--run` (см. `crate::headless::run_headless`). Добавить перед engine-fallback:
```rust
if args.first().map(String::as_str) == Some("--version") || args.first().map(String::as_str) == Some("-V") {
    println!("{APP_COMMAND} v{}", env!("CARGO_PKG_VERSION"));
    return Ok(());
}
```
(проверить точное имя константы имени бинаря — `APP_COMMAND` используется в `commands.rs::show_version`.)

- [ ] **Step 2: Проверить** —
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo build 2>&1 | tail -3
./target/debug/clave --version   # → clave vX.Y.Z, exit 0
```
- [ ] **Step 3: Commit** — `"clave: add --version flag"`

---

### Task 7: `format_dev_line` + команда `/dev` + busy-preflight (Rust)

**Files:**
- Create: `src/app/dev.rs` (модуль; `mod dev;` в `src/app/mod.rs`)
- Modify: `src/app/commands.rs` (ветка `/dev`, `command_has_handler`)
- Modify: `src/model/commands.rs` (запись палитры `/dev`)

**Interfaces:**
- Produces: `format_dev_line(raw: &str) -> String` (чистая; `CLAVE-DEV <type> <payload>` → строка с иконкой; необрамлённое → как есть); `App::start_dev(&mut self, task: String)` (Task 8).

- [ ] **Step 1: Тест (сперва)** — в `src/app/dev.rs`:
```rust
use super::*;

/// Превращает обрамлённую строку супервайзера в человекочитаемую строку транскрипта.
/// Необрамлённые строки (аномалия protocol-mode) возвращаются как есть — парсер не падает.
pub(crate) fn format_dev_line(raw: &str) -> String {
    let Some(rest) = raw.strip_prefix("CLAVE-DEV ") else {
        return raw.to_string();
    };
    let (type_, payload) = rest.split_once(' ').unwrap_or((rest, ""));
    let icon = match type_ {
        "progress" => "•",
        "log" => " ",
        "check" => "✓",
        "vision" => "◍",
        "diff" => "±",
        "report" => "⏺",
        "error" => "✗",
        _ => "·",
    };
    format!("{icon} {payload}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_formatted_by_type() {
        assert_eq!(format_dev_line("CLAVE-DEV progress раунд 1"), "• раунд 1");
        assert!(format_dev_line("CLAVE-DEV error боль").starts_with("✗"));
    }

    #[test]
    fn unframed_line_passes_through() {
        assert_eq!(format_dev_line("plain cargo output"), "plain cargo output");
    }
}
```

- [ ] **Step 2: Команда** — в `src/app/commands.rs` в `match command` добавить:
```rust
"/dev" => {
    if rest.trim().is_empty() {
        self.push_system(self.lang.choose("Использование: /dev <задача>", "Usage: /dev <task>"));
    } else {
        self.start_dev(rest.trim().to_string());
    }
}
```
Добавить `"/dev"` в `suppress_echo` (список запускающих) и в `command_has_handler`. В `src/model/commands.rs` — запись палитры `/dev` (usage `/dev <задача>`, описание «самопиление: внешний clave-dev на текущем repo»).

- [ ] **Step 3: `mod dev;`** в `src/app/mod.rs` рядом с другими под-модулями app.

- [ ] **Step 4: Проверить** — `cargo test format_dev_line every_palette_command_has_a_handler 2>&1 | tail` → PASS (в т.ч. palette↔handler инвариант).
- [ ] **Step 5: Commit** — `"clave-dev: /dev command, busy preflight hook, typed line formatter"`

---

### Task 8: Спавн супервайзера, типизированный стрим, отмена (`start_dev`)

**Files:**
- Modify: `src/app/dev.rs` (`start_dev`, резолв known-good/clave_dev/git-root, спавн)
- Create: `scripts/selfdev/mock-clave-dev.sh` (подставной супервайзер для headless-проверки)

**Interfaces:**
- Consumes: `spawn_worker`, `configure_process_group`, `kill_process_tree`, `spawn_reader`, `WorkerEvent`, `env::current_exe` (worker.rs); `format_dev_line` (Task 7).
- Produces: `App::start_dev`.

- [ ] **Step 1: Реализация `start_dev`** (образец — `start_task` из `runs.rs`). В `src/app/dev.rs`:
```rust
impl App {
    pub(crate) fn start_dev(&mut self, task: String) {
        if self.running {   // busy-preflight (§6)
            self.push_system(self.lang.choose("Clave уже выполняется.", "Clave is already running."));
            return;
        }
        let repo = self.resolved_work_dir();
        let git_root = match dev_git_root(&repo) {
            Some(root) => root,
            None => { self.push_system(self.lang.choose(
                "Не git-репозиторий — /dev работает в git-проекте.",
                "Not a git repo — /dev needs a git project.")); return; }
        };
        let (program, mut base_args) = match resolve_clave_dev(&git_root) {
            Some(inv) => inv,
            None => { self.push_system(self.lang.choose(
                "clave_dev не найден: задай CLAVE_DEV_HOME, поставь пакет или запусти из репо с tools/clave-dev.",
                "clave_dev not found: set CLAVE_DEV_HOME, install it, or run from a repo with tools/clave-dev.")); return; }
        };
        let known_good = match std::env::current_exe() {
            Ok(p) => p, Err(_) => { self.push_system("current_exe unavailable"); return; }
        };
        let (cancel_tx, cancel_rx) = mpsc::channel();
        self.running = true;
        self.run_started_at = Some(Instant::now());
        self.run_label = "clave-dev".to_string();
        self.cancel_tx = Some(cancel_tx);
        self.last_ctrl_c_at = None;
        self.status = self.lang.choose("самопиление", "self-dev").to_string();
        self.push_system(format!("◆ /dev {task}"));

        let effort = effort_label(self.effort_index).to_string();
        let rounds = self.rounds.to_string();
        let tx = self.tx.clone();
        base_args.extend([
            task, "--repo".into(), git_root.to_string_lossy().to_string(),
            "--known-good".into(), known_good.to_string_lossy().to_string(),
            "--protocol".into(), "clave-dev".into(),
            "--effort".into(), effort, "--rounds".into(), rounds,
        ]);
        spawn_worker(self.tx.clone(), move || {
            let mut command = Command::new(&program);
            command.current_dir(&git_root).args(&base_args)
                .stdout(Stdio::piped()).stderr(Stdio::piped());
            configure_process_group(&mut command);
            let mut child = match command.spawn() {
                Ok(c) => c,
                Err(err) => { let _ = tx.send(WorkerEvent::Failed(format!("spawn clave-dev: {err}"))); return; }
            };
            if let Some(out) = child.stdout.take() { spawn_dev_reader(out, tx.clone()); }
            if let Some(err) = child.stderr.take() { spawn_reader(err, tx.clone()); } // сырьё stderr как Line
            loop {
                if cancel_rx.try_recv().is_ok() {
                    kill_process_tree(&mut child);
                    let _ = tx.send(WorkerEvent::Cancelled);
                    return;
                }
                match child.try_wait() {
                    Ok(Some(status)) => { let _ = tx.send(WorkerEvent::Done(status.code().unwrap_or(1))); return; }
                    Ok(None) => thread::sleep(Duration::from_millis(80)),
                    Err(err) => { let _ = tx.send(WorkerEvent::Failed(format!("wait: {err}"))); return; }
                }
            }
        });
    }
}

/// Читатель stdout супервайзера: каждую строку прогоняем через format_dev_line (§5).
fn spawn_dev_reader<R: io::Read + Send + 'static>(reader: R, tx: Sender<WorkerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(reader);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx.send(WorkerEvent::Line(format_dev_line(&line)));
        }
    });
}

fn dev_git_root(path: &std::path::Path) -> Option<PathBuf> {
    let out = Command::new("git").arg("-C").arg(path).args(["rev-parse", "--show-toplevel"])
        .output().ok()?;
    if !out.status.success() { return None; }
    Some(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

/// Резолв пакета clave_dev (спека §4): CLAVE_DEV_HOME → <git root>/tools/clave-dev → модуль.
fn resolve_clave_dev(git_root: &std::path::Path) -> Option<(String, Vec<String>)> {
    let py = "python3".to_string();
    if let Ok(home) = std::env::var("CLAVE_DEV_HOME") {
        return Some((py, vec!["-m".into(), "clave_dev".into(), pythonpath_prefix(&home)]));
    }
    let repo_pkg = git_root.join("tools").join("clave-dev");
    if repo_pkg.join("clave_dev").is_dir() {
        return Some((py, vec!["-m".into(), "clave_dev".into(), pythonpath_prefix(&repo_pkg.to_string_lossy())]));
    }
    // Установленный модуль: проверяем импортируемость.
    let importable = Command::new("python3").args(["-c", "import clave_dev"]).status()
        .map(|s| s.success()).unwrap_or(false);
    importable.then(|| (py, vec!["-m".into(), "clave_dev".into()]))
}
```
> Примечание исполнителю: `pythonpath_prefix` — не аргумент, а установка `PYTHONPATH` на `Command` (env). В коде выше упрощено; реально: не добавлять в args, а `command.env("PYTHONPATH", home)` в теле воркера (пробросить выбранный `home` в замыкание). Держать один способ: вернуть `(program, args, Option<pythonpath>)` и выставить env перед spawn.

- [ ] **Step 2: Подставной супервайзер** — `scripts/selfdev/mock-clave-dev.sh`:
```bash
#!/bin/bash
# Мок внешнего clave-dev для headless-проверки /dev: печатает типизированные строки и выходит 0.
echo "CLAVE-DEV progress раунд 1: агент правит"
echo "CLAVE-DEV check {\"name\":\"build\",\"ok\":true}"
echo "CLAVE-DEV report {\"converged\":true,\"rounds\":1,\"max_rounds\":1,\"worktree\":\"/tmp/wt\"}"
exit 0
```

- [ ] **Step 3: Verify (headless, pyte + мок).** Собрать; в pty-харнессе (как `scripts/render_check.py`) задать `CLAVE_DEV_HOME`/подставить `python3`→мок через PATH, ввести `/dev поправь футер`, убедиться: в транскрипте видно «• раунд 1…», «✓ …build…», «⏺ …», статус вернулся из busy; ввод `/dev` во время busy → «Clave уже выполняется». Отмена: длинный мок + Ctrl+C → «остановлено», нет висящих детей.
```bash
export PATH="$HOME/.cargo/bin:$PATH"; cargo build 2>&1 | tail -3
cargo test 2>&1 | tail -5   # регрессия Rust
```
- [ ] **Step 4: Commit** — `"clave-dev: spawn external supervisor from /dev with typed streaming and cancel"`

---

## Self-Review

- **Покрытие спеки:** §3 known-good — Task 3 (`--version`+sha256) + Task 6 (`clave --version`) + Task 8 (абсолютный `current_exe`); §4 резолв/ git-root — Task 4 + Task 8 (`resolve_clave_dev`/`dev_git_root`); §5 типы/дисциплина — Task 1 (эмиттер) + Task 5 (проводка) + Task 7 (`format_dev_line`) + Task 8 (stdout через reader, stderr отдельно); §6 команда/busy/стрим/отмена — Task 7 (команда+busy) + Task 8 (спавн+cancel); §7 конфиг — Task 8 (effort/rounds); §8 ошибки — внятные сообщения в Task 8 (не git/не найден/спавн), §9 — юниты + headless-verify (Task 8 Step 3).
- **Плейсхолдеры:** один намеренный узел — `pythonpath_prefix`/проброс `PYTHONPATH` помечен примечанием исполнителю с точным способом (env на Command). Устранить при реализации Task 8.
- **Согласованность типов:** `format_dev_line` (Task 7) — единственная точка форматирования, потребляется `spawn_dev_reader` (Task 8); `Emitter` (Task 1) типы совпадают с матчем иконок в `format_dev_line`; `WorkerEvent::Line/Done/Cancelled/Failed` уже обрабатываются в `events.rs` — новые пути не требуют новых вариантов; busy-preflight через существующий `self.running`.
- **Риск:** Task 8 — самый тяжёлый (реальная инфра потоков/спавна); проверяется headless на моке. Полный e2e (реальные cargo-сборки из TUI) — ручной, тяжёлый.

## Границы

Закрывает продуктовую интеграцию v1-цикла. Хвост (авто-финализация, произвольный repo, параллельные кандидаты) — отдельными циклами. В main self-dev сливается только после завершения и проверки всего v1-цикла на ветке.
