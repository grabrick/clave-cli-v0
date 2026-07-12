# clave-dev — Plan 1: headless-вход `clave --run` (Implementation Plan)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Добавить в `clave` неинтерактивный режим `clave --run tandem "<task>"`, который запускает существующий `run_tandem` без TUI, стримит активность в stdout и печатает машинную финальную строку `CLAVE-RUN <json>` — чтобы внешний супервайзер (Plan 2) мог запускать агента скриптом.

**Architecture:** Новый модуль `src/headless.rs` переиспользует worker-функцию `run_tandem` (она уже standalone: принимает параметры и общается через `mpsc`-канал `WorkerEvent`, не зависит от `App`/TUI). headless парсит аргументы, берёт роли/effort из `AppConfig`, делает auth-preflight, спавнит `run_tandem` в потоке, дренит события в stdout и по результату печатает `CLAVE-RUN <json>` + код выхода. Никакого raw-mode/alt-screen/интерактива.

**Tech Stack:** Rust 2021; `std::sync::mpsc`, `std::thread`; `serde_json` (уже в зависимостях) для финальной строки.

## Global Constraints

- Переиспользовать ЯДРО (`run_tandem` из `src/worker.rs`), не тянуть в headless ничего из `src/runtime.rs`/`src/render.rs`/`App`. (Спека §3.)
- Контракт (спека §3): `exit 0` = «агент отработал», НЕ «задача решена». Коды: `0` завершено, `2` провайдер не залогинен (preflight), `3` сбой оркестрации, `1` прочее (ошибки разбора аргументов/задачи).
- stdout: построчный прогресс, затем **ровно одна** строка `CLAVE-RUN <json>` с полями `{status, code, provider, usage, ended_reason}`.
- v1 поддерживает только `mode == "tandem"`; прочие режимы → ошибка (exit 1).
- Комментарии в коде — на русском (стиль репозитория); идентификаторы/строки протокола (`CLAVE-RUN`, ключи json) — на английском.
- Проверки перед коммитом каждой задачи: `cargo test` (зелёно), `cargo fmt`, `cargo clippy --all-targets -- -D warnings` (0).

---

### Task 1: Разбор аргументов `--run`

**Files:**
- Create: `src/headless.rs`
- Modify: `src/main.rs` (добавить `mod headless;` и реэкспорт)

**Interfaces:**
- Produces: `pub(crate) struct RunArgs { mode: String, cwd: Option<String>, effort: Option<String>, rounds: Option<usize>, task_stdin: bool, task: Option<String> }` и `pub(crate) fn parse_run_args(args: &[String]) -> Result<RunArgs, String>`.

- [ ] **Step 1: Создать `src/headless.rs` с типом и функцией разбора + падающий тест**

Создай `src/headless.rs`:

```rust
use crate::prelude::*;
use crate::*;

/// Разобранные аргументы `clave --run <mode> [флаги] [-- <task>]`.
#[derive(Debug, PartialEq)]
pub(crate) struct RunArgs {
    pub(crate) mode: String,
    pub(crate) cwd: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) rounds: Option<usize>,
    pub(crate) task_stdin: bool,
    pub(crate) task: Option<String>,
}

/// Индексный разбор (без хитрых move у итераторов): первый аргумент — режим, дальше
/// флаги; `--` или первый позиционный аргумент забирают остаток как текст задачи.
pub(crate) fn parse_run_args(args: &[String]) -> Result<RunArgs, String> {
    let mode = args
        .first()
        .ok_or("--run: не задан режим (например, tandem)")?
        .clone();
    let mut out = RunArgs {
        mode,
        cwd: None,
        effort: None,
        rounds: None,
        task_stdin: false,
        task: None,
    };
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--cwd" => {
                i += 1;
                out.cwd = Some(args.get(i).ok_or("--cwd требует значение")?.clone());
            }
            "--effort" => {
                i += 1;
                out.effort = Some(args.get(i).ok_or("--effort требует значение")?.clone());
            }
            "--rounds" => {
                i += 1;
                let v = args.get(i).ok_or("--rounds требует значение")?;
                out.rounds = Some(v.parse().map_err(|_| format!("--rounds: не число: {v}"))?);
            }
            "--task-stdin" => out.task_stdin = true,
            "--" => {
                out.task = Some(args[i + 1..].join(" "));
                break;
            }
            other if other.starts_with('-') => {
                return Err(format!("--run: неизвестный флаг {other}"));
            }
            _ => {
                out.task = Some(args[i..].join(" "));
                break;
            }
        }
        i += 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_mode_flags_and_positional_task() {
        let got = parse_run_args(&v(&[
            "tandem", "--cwd", "/tmp/wt", "--effort", "high", "--rounds", "2", "fix the footer",
        ]))
        .unwrap();
        assert_eq!(got.mode, "tandem");
        assert_eq!(got.cwd.as_deref(), Some("/tmp/wt"));
        assert_eq!(got.effort.as_deref(), Some("high"));
        assert_eq!(got.rounds, Some(2));
        assert!(!got.task_stdin);
        assert_eq!(got.task.as_deref(), Some("fix the footer"));
    }

    #[test]
    fn double_dash_takes_rest_as_task_even_with_leading_dash() {
        let got = parse_run_args(&v(&["tandem", "--", "--weird task-name"])).unwrap();
        assert_eq!(got.task.as_deref(), Some("--weird task-name"));
    }

    #[test]
    fn task_stdin_flag_and_no_task() {
        let got = parse_run_args(&v(&["tandem", "--task-stdin"])).unwrap();
        assert!(got.task_stdin);
        assert_eq!(got.task, None);
    }

    #[test]
    fn errors_on_unknown_flag_and_missing_value() {
        assert!(parse_run_args(&v(&["tandem", "--nope"])).is_err());
        assert!(parse_run_args(&v(&["tandem", "--cwd"])).is_err());
        assert!(parse_run_args(&v(&[])).is_err());
    }
}
```

Добавь в `src/main.rs` объявление модуля и реэкспорт (рядом с прочими):

```rust
mod headless;
```
и в блоке реэкспортов:
```rust
pub(crate) use headless::*;
```

- [ ] **Step 2: Запустить тесты — убедиться, что падают до компиляции модуля / проходят после**

Run: `cargo test headless::tests -- --nocapture`
Expected: компилируется и `parses_mode_flags_and_positional_task`, `double_dash_takes_rest_as_task_even_with_leading_dash`, `task_stdin_flag_and_no_task`, `errors_on_unknown_flag_and_missing_value` — PASS.

- [ ] **Step 3: fmt/clippy**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings`
Expected: без изменений форматирования / 0 предупреждений.

- [ ] **Step 4: Commit**

```bash
git add src/headless.rs src/main.rs
git commit -m "Add headless --run argument parsing"
```

---

### Task 2: Вывод параметров запуска из конфига

**Files:**
- Modify: `src/headless.rs`

**Interfaces:**
- Consumes: `RunArgs` (Task 1); `AppConfig` (поля `mode: Mode`, `lang: Language`, `rounds: usize`, `work_dir: String`, `effort_index: usize`); `Mode::architect_provider(self) -> Provider`, `Mode::reviewer_provider(self) -> Provider`; `Provider::as_str(self) -> &'static str`; `effort_label(index: usize) -> &'static str`; `resolve_work_dir(&str, &Path) -> PathBuf`; `launch_work_dir() -> PathBuf`.
- Produces: `pub(crate) struct RunParams { executor: &'static str, critic: &'static str, effort: String, rounds: usize, work_dir: PathBuf, lang: Language, executor_provider: Provider, critic_provider: Provider }` и `pub(crate) fn resolve_run_params(config: &AppConfig, args: &RunArgs) -> RunParams`.

- [ ] **Step 1: Добавить `RunParams` и `resolve_run_params` + падающий тест**

В `src/headless.rs` (перед `#[cfg(test)]`):

```rust
/// Готовые параметры для `run_tandem`, выведенные из конфига и аргументов.
pub(crate) struct RunParams {
    pub(crate) executor: &'static str,
    pub(crate) critic: &'static str,
    pub(crate) effort: String,
    pub(crate) rounds: usize,
    pub(crate) work_dir: PathBuf,
    pub(crate) lang: Language,
    pub(crate) executor_provider: Provider,
    pub(crate) critic_provider: Provider,
}

/// Роли — из `Mode`, effort — из `--effort` или общего значения конфига (v1: одно
/// значение на обе роли; раздельный per-role effort — позже), рабочий каталог — из
/// `--cwd` или конфига.
pub(crate) fn resolve_run_params(config: &AppConfig, args: &RunArgs) -> RunParams {
    let executor_provider = config.mode.architect_provider();
    let critic_provider = config.mode.reviewer_provider();
    let effort = args
        .effort
        .clone()
        .unwrap_or_else(|| effort_label(config.effort_index).to_string());
    let rounds = args.rounds.unwrap_or(config.rounds);
    let work_dir = match &args.cwd {
        Some(dir) => PathBuf::from(dir),
        None => resolve_work_dir(&config.work_dir, &launch_work_dir()),
    };
    RunParams {
        executor: executor_provider.as_str(),
        critic: critic_provider.as_str(),
        effort,
        rounds,
        work_dir,
        lang: config.lang,
        executor_provider,
        critic_provider,
    }
}
```

В `mod tests` добавь:

```rust
    #[test]
    fn resolves_roles_effort_and_cwd_from_config_and_args() {
        let mut config = AppConfig::default();
        config.mode = Mode::ClaudeCodex; // архитектор claude, ревьюер codex
        config.effort_index = 2; // "high"
        config.rounds = 3;
        let args = parse_run_args(&v(&["tandem", "--cwd", "/tmp/wt"])).unwrap();

        let params = resolve_run_params(&config, &args);
        assert_eq!(params.executor, "claude");
        assert_eq!(params.critic, "codex");
        assert_eq!(params.effort, effort_label(2)); // общий effort из конфига
        assert_eq!(params.rounds, 3);
        assert_eq!(params.work_dir, PathBuf::from("/tmp/wt"));
    }

    #[test]
    fn effort_and_rounds_flags_override_config() {
        let config = AppConfig::default();
        let args = parse_run_args(&v(&["tandem", "--effort", "max", "--rounds", "5"])).unwrap();
        let params = resolve_run_params(&config, &args);
        assert_eq!(params.effort, "max");
        assert_eq!(params.rounds, 5);
    }
```

- [ ] **Step 2: Запустить тесты — убедиться, что проходят**

Run: `cargo test headless::tests::resolves_roles_effort_and_cwd_from_config_and_args headless::tests::effort_and_rounds_flags_override_config -- --nocapture`
Expected: обе PASS. Если `AppConfig` не имеет `Default` — использовать существующий конструктор дефолта из `src/app/config.rs` (проверь `impl Default for AppConfig`).

- [ ] **Step 3: fmt/clippy**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings`
Expected: 0 предупреждений.

- [ ] **Step 4: Commit**

```bash
git add src/headless.rs
git commit -m "Derive tandem run params from config and args"
```

---

### Task 3: Финальная строка `CLAVE-RUN <json>` и печать событий

**Files:**
- Modify: `src/headless.rs`

**Interfaces:**
- Consumes: `WorkerEvent` (варианты `Line/ChatLine/StreamDelta/ReasoningDelta/Activity/Done/ChatDone/PlanReady/Cancelled/Failed/AuthMissing`); `TandemResult { Completed(i32, Option<RunUsage>), Cancelled }`; `RunUsage { input, output, cache_read, cache_creation, cost_usd }`.
- Produces: `fn print_event(event: &WorkerEvent)`; `fn final_line(result: &io::Result<TandemResult>, provider: &str) -> (String, i32)`.

- [ ] **Step 1: Добавить `print_event` и `final_line` + падающий тест**

В `src/headless.rs` (перед `#[cfg(test)]`):

```rust
/// Печатает событие агента в stdout. Инкременты стрима — без перевода строки; готовые
/// строки — построчно. Терминальные события агрегируются в результат, здесь не печатаются.
fn print_event(event: &WorkerEvent) {
    match event {
        WorkerEvent::Line(s) | WorkerEvent::ChatLine(s) | WorkerEvent::Activity(s) => {
            println!("{s}");
        }
        WorkerEvent::StreamDelta(s) | WorkerEvent::ReasoningDelta(s) => {
            print!("{s}");
        }
        WorkerEvent::Done(_)
        | WorkerEvent::ChatDone(..)
        | WorkerEvent::PlanReady(..)
        | WorkerEvent::Cancelled
        | WorkerEvent::Failed(_)
        | WorkerEvent::AuthMissing(_) => {}
    }
    let _ = io::stdout().flush();
}

/// Строит машинную финальную строку `CLAVE-RUN <json>` и код выхода процесса.
/// exit 0 = агент отработал (включая cancelled); 3 = сбой оркестрации.
fn final_line(result: &io::Result<TandemResult>, provider: &str) -> (String, i32) {
    match result {
        Ok(TandemResult::Completed(code, usage)) => {
            let usage_json = usage.as_ref().map(|u| {
                serde_json::json!({
                    "input": u.input,
                    "output": u.output,
                    "cache_read": u.cache_read,
                    "cache_creation": u.cache_creation,
                    "cost_usd": u.cost_usd,
                })
            });
            let json = serde_json::json!({
                "status": "completed",
                "code": code,
                "provider": provider,
                "usage": usage_json,
                "ended_reason": "completed",
            });
            (format!("CLAVE-RUN {json}"), 0)
        }
        Ok(TandemResult::Cancelled) => (
            format!(
                "CLAVE-RUN {}",
                serde_json::json!({"status":"cancelled","provider":provider,"ended_reason":"cancelled"})
            ),
            0,
        ),
        Err(err) => (
            format!(
                "CLAVE-RUN {}",
                serde_json::json!({"status":"error","provider":provider,"ended_reason":err.to_string()})
            ),
            3,
        ),
    }
}
```

В `mod tests` добавь:

```rust
    #[test]
    fn final_line_completed_is_exit_0_and_parseable_json() {
        let (line, code) = final_line(&Ok(TandemResult::Completed(0, None)), "claude");
        assert_eq!(code, 0);
        let json = line.strip_prefix("CLAVE-RUN ").expect("префикс CLAVE-RUN");
        let value: serde_json::Value = serde_json::from_str(json).expect("валидный json");
        assert_eq!(value["status"], "completed");
        assert_eq!(value["code"], 0);
        assert_eq!(value["provider"], "claude");
    }

    #[test]
    fn final_line_error_is_exit_3() {
        let err = io::Error::new(io::ErrorKind::Other, "boom");
        let (line, code) = final_line(&Err(err), "codex");
        assert_eq!(code, 3);
        let value: serde_json::Value =
            serde_json::from_str(line.strip_prefix("CLAVE-RUN ").unwrap()).unwrap();
        assert_eq!(value["status"], "error");
    }
```

- [ ] **Step 2: Запустить тесты**

Run: `cargo test headless::tests::final_line_completed_is_exit_0_and_parseable_json headless::tests::final_line_error_is_exit_3 -- --nocapture`
Expected: обе PASS.

- [ ] **Step 3: fmt/clippy**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings`
Expected: 0 предупреждений. (Если clippy ругается на `print_event` как `dead_code` до Task 4 — временно допустимо; Task 4 подключит её. Проверь, что после Task 4 предупреждения нет.)

- [ ] **Step 4: Commit**

```bash
git add src/headless.rs
git commit -m "Emit CLAVE-RUN result line and print agent events"
```

---

### Task 4: Связать `run_headless` и подключить `--run` в `main_entry`

**Files:**
- Modify: `src/headless.rs` (функция-точка входа `run_headless`)
- Modify: `src/runtime.rs` (`main_entry`: ветка `--run`)

**Interfaces:**
- Consumes: `parse_run_args`, `resolve_run_params`, `print_event`, `final_line` (Tasks 1–3); `provider_authenticated(Provider) -> bool`; `run_tandem(executor, critic, executor_effort, critic_effort, task, rounds, work_dir, cancel_rx, tx, lang) -> io::Result<TandemResult>`; `load_config(&Path) -> AppConfig`; `config_path() -> PathBuf`.
- Produces: `pub(crate) fn run_headless(args: &[String]) -> AnyResult<()>`.

- [ ] **Step 1: Добавить `run_headless` в `src/headless.rs`**

```rust
/// Точка входа headless-режима. Возвращает `Err` только на ошибках разбора/входа
/// (main напечатает и выйдет с кодом 1); успешный путь завершается `process::exit`.
pub(crate) fn run_headless(args: &[String]) -> AnyResult<()> {
    let parsed = parse_run_args(args).map_err(|e| -> Box<dyn Error> { e.into() })?;
    if parsed.mode != "tandem" {
        return Err(format!(
            "--run: v1 поддерживает только режим 'tandem', получено '{}'",
            parsed.mode
        )
        .into());
    }

    let task = if parsed.task_stdin {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        buf
    } else {
        parsed
            .task
            .clone()
            .ok_or("--run: задача не задана (позиционный аргумент, `--` или --task-stdin)")?
    };

    let config = load_config(&config_path());
    let params = resolve_run_params(&config, &parsed);

    // Auth-preflight (спека §3): не залогинен → exit 2, без попытки правок.
    for (provider_enum, provider_str) in [
        (params.executor_provider, params.executor),
        (params.critic_provider, params.critic),
    ] {
        if !provider_authenticated(provider_enum) {
            println!(
                "CLAVE-RUN {}",
                serde_json::json!({
                    "status": "auth_missing",
                    "provider": provider_str,
                    "ended_reason": "provider not logged in",
                })
            );
            std::process::exit(2);
        }
    }

    // Запускаем run_tandem в потоке, дренируем события в stdout, забираем результат.
    let (tx, rx) = mpsc::channel();
    let (_cancel_tx, cancel_rx) = mpsc::channel::<()>(); // headless: без интерактивной отмены
    let executor = params.executor;
    let critic = params.critic;
    let effort = params.effort.clone();
    let task_run = task.clone();
    let rounds = params.rounds;
    let work_dir = params.work_dir.clone();
    let lang = params.lang;
    let handle = thread::spawn(move || {
        run_tandem(
            executor, critic, &effort, &effort, &task_run, rounds, &work_dir, cancel_rx, tx, lang,
        )
    });

    for event in rx {
        print_event(&event);
    }
    let result = handle
        .join()
        .unwrap_or_else(|_| Ok(TandemResult::Cancelled));

    let (line, code) = final_line(&result, params.executor);
    println!("{line}");
    std::process::exit(code);
}
```

- [ ] **Step 2: Подключить ветку `--run` в `main_entry` (`src/runtime.rs`)**

В `main_entry`, СРАЗУ после блока `-h/--help` и ДО guard'а неизвестных флагов, добавь:

```rust
    if args.first().map(String::as_str) == Some("--run") {
        return run_headless(&args[1..]);
    }
```

Итоговое начало `main_entry`:
```rust
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_usage();
        return Ok(());
    }

    if args.first().map(String::as_str) == Some("--run") {
        return run_headless(&args[1..]);
    }

    // Неизвестный флаг (например, удалённый `--serve`) ...
    if let Some(first) = args.first() {
        if first.starts_with('-') {
```

- [ ] **Step 3: Собрать; проверить, что `--run` без задачи даёт понятную ошибку и код 1**

Run: `cargo build --release && CLAVE_HOME="$(mktemp -d)" CLAVE_SKIP_ONBOARDING=1 target/release/clave --run tandem ; echo "exit=$?"`
Expected: печатает ошибку `--run: задача не задана ...` в stderr и `exit=1` (Err из `run_headless` всплывает в `main`). Ветка `--run` не уходит в guard/движок.

- [ ] **Step 4: fmt/clippy/test**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: 0 предупреждений; тесты (включая прежние 99+ и headless-юниты) — зелёные.

- [ ] **Step 5: Commit**

```bash
git add src/headless.rs src/runtime.rs
git commit -m "Wire clave --run headless tandem entry"
```

---

### Task 5: Смоук end-to-end с мок-провайдерами

**Files:**
- Create: `scripts/selfdev/mock-codex.sh`, `scripts/selfdev/mock-claude.sh`, `scripts/selfdev/smoke_headless.sh`

**Interfaces:**
- Consumes: собранный `target/release/clave` (Task 4); мок-провайдеры через `CLAVE_CLAUDE`/`CLAVE_CODEX`.

Мок-скрипты повторяют контракт CLI провайдеров ровно настолько, чтобы `run_tandem` прошёл: отвечают на auth-пробу и на вызов, ничего реального не дёргая.

- [ ] **Step 1: Создать мок-провайдеры**

`scripts/selfdev/mock-codex.sh`:
```bash
#!/bin/bash
# Мок codex для смоука headless: auth-проба + краткий ответ (в файл -o) + usage JSONL.
case "$*" in
  *"login status"*) echo "Logged in as mock-codex"; exit 0 ;;
esac
outfile=""; prev=""
for a in "$@"; do
  [ "$prev" = "-o" ] && outfile="$a"
  prev="$a"
done
[ -n "$outfile" ] && printf 'mock codex answer\n' > "$outfile"
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":5}}'
exit 0
```

`scripts/selfdev/mock-claude.sh`:
```bash
#!/bin/bash
# Мок claude: auth-проба + stream-json ответ.
case "$*" in
  *"auth status"*) echo "logged in as mock-claude"; exit 0 ;;
esac
printf '%s\n' '{"type":"content_block_delta","delta":{"type":"text_delta","text":"mock claude answer"}}'
printf '%s\n' '{"type":"result","result":"mock claude answer","is_error":false,"usage":{"input_tokens":10,"output_tokens":5},"total_cost_usd":0.001}'
exit 0
```

- [ ] **Step 2: Создать смоук-скрипт**

`scripts/selfdev/smoke_headless.sh`:
```bash
#!/bin/bash
# Прогоняет `clave --run tandem` на мок-провайдерах и проверяет контракт:
# в stdout есть ровно одна строка CLAVE-RUN с валидным json и exit 0.
set -u
here="$(cd "$(dirname "$0")" && pwd)"
bin="${1:?путь к target/release/clave}"
home="$(mktemp -d)"
wt="$(mktemp -d)"
out="$(CLAVE_HOME="$home" CLAVE_SKIP_ONBOARDING=1 \
  CLAVE_CLAUDE="$here/mock-claude.sh" CLAVE_CODEX="$here/mock-codex.sh" \
  "$bin" --run tandem --cwd "$wt" --rounds 1 "smoke task" 2>/dev/null)"
code=$?
echo "$out"
final="$(printf '%s\n' "$out" | grep -c '^CLAVE-RUN ')"
json="$(printf '%s\n' "$out" | grep '^CLAVE-RUN ' | tail -1 | sed 's/^CLAVE-RUN //')"
python3 -c "import json,sys; json.loads(sys.argv[1])" "$json" || { echo "FAIL: невалидный json"; exit 1; }
[ "$final" = "1" ] || { echo "FAIL: ожидалась ровно одна строка CLAVE-RUN, получено $final"; exit 1; }
[ "$code" = "0" ] || { echo "FAIL: exit=$code, ожидался 0"; exit 1; }
echo "OK: headless smoke passed (exit 0, one CLAVE-RUN line, valid json)"
```

- [ ] **Step 3: Сделать исполняемыми и прогнать смоук**

Run:
```bash
chmod +x scripts/selfdev/*.sh
cargo build --release
scripts/selfdev/smoke_headless.sh target/release/clave
```
Expected: печатается активность мок-агента и `OK: headless smoke passed (exit 0, one CLAVE-RUN line, valid json)`.

- [ ] **Step 4: Commit**

```bash
git add scripts/selfdev/mock-codex.sh scripts/selfdev/mock-claude.sh scripts/selfdev/smoke_headless.sh
git commit -m "Add headless smoke test with mock providers"
```

---

## Что дальше (Plan 2)

Plan 1 даёт самодостаточный, тестируемый headless-вход. **Plan 2 — внешний Python-супервайзер** (`tools/clave-dev/`): worktree/preflight (§7), изоляция бинарей/PATH (§6), checks-парсинг, observer+assertions (§5), петля и отчёт (§4/§8), смоук петли на моках (§9). Он потребляет `clave --run` из этого плана.

## Self-Review

- **Покрытие спеки (для Plan 1):** §3 контракт — Tasks 1–4 (инвокация, stdin/stdout, коды, `CLAVE-RUN <json>`, auth-preflight, «exit 0 ≠ решено» — final_line не судит успех); §9 смоук на моках — Task 5. Части §4–§8 (петля, observer, изоляция, worktree, отчёт) относятся к супервайзеру → **Plan 2** (явно указано). Пробелов внутри объёма Plan 1 нет.
- **Плейсхолдеры:** нет — код приведён полностью в каждом шаге; тесты с реальными ассертами.
- **Согласованность типов:** `RunArgs`/`RunParams` — поля совпадают между Tasks 1–4; `run_tandem` вызывается точной сигнатурой (executor/critic `&'static str`, два effort `&str`, task `&str`, rounds `usize`, work_dir `&Path`, cancel_rx, tx, lang); `provider_authenticated(Provider)`; `TandemResult::Completed(i32, Option<RunUsage>)` — сопоставление по ссылке в `final_line`. `RunUsage`-поля (`input/output/cache_read/cache_creation/cost_usd`) — как в `src/model/usage.rs`.
- **Замечание для исполнителя:** если `AppConfig` не выводит `Default`, взять существующий дефолт-конструктор из `src/app/config.rs`; если `run_tandem`/`TandemResult` сместились по строкам — сверить сигнатуру, не менять контракт.
