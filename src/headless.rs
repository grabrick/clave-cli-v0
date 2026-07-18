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

/// Роли — из `Mode`; effort — из `--effort` или общего значения конфига (v1: одно
/// значение на обе роли, per-role effort — позже); рабочий каталог — из `--cwd` или
/// конфига.
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
        // headless неинтерактивен: гейт «нет консенсуса» некому подтвердить, воркер
        // авто-исполняет (см. предзагрузку Execute в run_headless). Печатаем, чтобы в
        // логах self-dev было видно, что консенсуса не было.
        WorkerEvent::TandemNeedsApproval => {
            println!("⚠ Консенсус не достигнут — headless исполняет последнюю версию.");
        }
        // Исполнитель просит уточнений, но в headless отвечать некому — воркер идёт дальше
        // (канал ввода закрыт → Disconnected). Печатаем, чтобы это было видно в логах.
        WorkerEvent::TandemNeedsInput(_) => {
            println!("⚠ Исполнителю нужны уточнения — в headless некому ответить, продолжаю.");
        }
        // TandemStepEnd — сигнал живому региону UI фиксировать шаг; в headless региона нет
        // (шаги печатаются построчно сразу), печатать нечего.
        WorkerEvent::TandemStepEnd => {}
        WorkerEvent::Done(_)
        | WorkerEvent::ChatDone(..)
        | WorkerEvent::PlanReady(..)
        | WorkerEvent::Cancelled
        | WorkerEvent::Failed(_)
        | WorkerEvent::AuthMissing(_)
        // События панели /plugins в headless-прогоне не возникают — печатать нечего.
        | WorkerEvent::PluginsLoaded(_)
        | WorkerEvent::PluginActionDone
        | WorkerEvent::MarketplacesLoaded(_)
        | WorkerEvent::MarketplaceActionDone => {}
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
                serde_json::json!({"status": "cancelled", "provider": provider, "ended_reason": "cancelled"})
            ),
            0,
        ),
        Err(err) => (
            format!(
                "CLAVE-RUN {}",
                serde_json::json!({"status": "error", "provider": provider, "ended_reason": err.to_string()})
            ),
            3,
        ),
    }
}

/// Точка входа headless-режима. Возвращает `Err` только на ошибках разбора/входа
/// (main напечатает и выйдет с кодом 1); успешный путь завершается `process::exit`.
pub(crate) fn run_headless(args: &[String]) -> AnyResult<()> {
    let parsed =
        parse_run_args(args).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    if parsed.mode != "tandem" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "--run: v1 поддерживает только режим 'tandem', получено '{}'",
                parsed.mode
            ),
        )
        .into());
    }

    let task = if parsed.task_stdin {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        buf
    } else {
        parsed.task.clone().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "--run: задача не задана (позиционный аргумент, `--` или --task-stdin)",
            )
        })?
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
    // headless неинтерактивен: гейт «нет консенсуса» некому подтвердить. Предзагружаем
    // Execute — это в точности прежнее поведение (нет консенсуса → исполнить последнюю),
    // на которое настроен self-dev гейт; иначе воркер завис бы на gate_rx навсегда.
    let (gate_tx, gate_rx) = mpsc::channel();
    gate_tx
        .send(TandemGate::Execute)
        .expect("предзагрузка решения гейта");
    // Ввод-гейт «нужны уточнения» тоже некому обслужить: сразу закрываем канал ответа →
    // recv даёт Disconnected → воркер идёт дальше, а не виснет (та же логика, что у gate).
    let (input_tx, input_rx) = mpsc::channel::<String>();
    drop(input_tx);
    let handle = thread::spawn(move || {
        run_tandem(
            executor, critic, &effort, &effort, &task_run, rounds, &work_dir, cancel_rx, gate_rx,
            input_rx, tx, lang,
        )
    });

    for event in rx {
        print_event(&event);
    }
    let result = handle.join().unwrap_or(Ok(TandemResult::Cancelled));

    let (line, code) = final_line(&result, params.executor);
    println!("{line}");
    std::process::exit(code);
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
            "tandem",
            "--cwd",
            "/tmp/wt",
            "--effort",
            "high",
            "--rounds",
            "2",
            "fix the footer",
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

    #[test]
    fn resolves_roles_effort_and_cwd_from_config_and_args() {
        let config = AppConfig {
            mode: Mode::ClaudeCodex, // архитектор claude, ревьюер codex
            effort_index: 2,         // "high"
            rounds: 3,
            ..AppConfig::default()
        };
        let args = parse_run_args(&v(&["tandem", "--cwd", "/tmp/wt"])).unwrap();

        let params = resolve_run_params(&config, &args);
        assert_eq!(params.executor, "claude");
        assert_eq!(params.critic, "codex");
        assert_eq!(params.effort, effort_label(2));
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
        let err = io::Error::other("boom");
        let (line, code) = final_line(&Err(err), "codex");
        assert_eq!(code, 3);
        let value: serde_json::Value =
            serde_json::from_str(line.strip_prefix("CLAVE-RUN ").unwrap()).unwrap();
        assert_eq!(value["status"], "error");
    }
}
