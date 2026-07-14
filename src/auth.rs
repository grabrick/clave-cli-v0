use crate::prelude::*;
use crate::*;

pub(crate) fn auth_requirements_ready(mode: Mode, onboarding: &Onboarding) -> bool {
    (!mode.needs_codex() || onboarding.codex_authenticated)
        && (!mode.needs_claude() || onboarding.claude_authenticated)
}

pub(crate) fn missing_auth_text(mode: Mode, onboarding: &Onboarding, lang: Language) -> String {
    let mut missing = Vec::new();
    if mode.needs_codex() && !onboarding.codex_authenticated {
        missing.push(if onboarding.codex_installed {
            "Codex"
        } else {
            lang.choose("Codex CLI не найден", "Codex CLI missing")
        });
    }
    if mode.needs_claude() && !onboarding.claude_authenticated {
        missing.push(if onboarding.claude_installed {
            "Claude"
        } else {
            lang.choose("Claude CLI не найден", "Claude CLI missing")
        });
    }

    if missing.is_empty() {
        lang.choose("всё готово", "all ready").to_string()
    } else {
        missing.join(" + ")
    }
}

/// Проверяет логин ОДНОГО провайдера (для воркера, без заморозки UI).
pub(crate) fn provider_authenticated(provider: Provider) -> bool {
    match provider {
        Provider::Claude => claude_auth_probe().authenticated,
        Provider::Codex => codex_auth_probe().authenticated,
    }
}

/// Лёгкая проверка наличия бинарника провайдера в PATH — без запуска процесса,
/// поэтому безопасно звать из UI-потока (в отличие от `*_auth_probe`).
pub(crate) fn provider_binary_present(provider: &str) -> bool {
    let name = match provider {
        "claude" => claude_binary(),
        "codex" => codex_binary(),
        _ => return false,
    };
    if name.contains('/') {
        return std::path::Path::new(&name).is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(&name).is_file()))
}

pub(crate) fn codex_auth_probe() -> AuthProbe {
    auth_probe(&codex_binary(), &["login", "status"], auth_timeout())
}

pub(crate) fn claude_auth_probe() -> AuthProbe {
    auth_probe(
        &claude_binary(),
        &["auth", "status", "--text"],
        auth_timeout(),
    )
}

/// Потолок ожидания пробы логина. Пробу зовут СИНХРОННО: из `App::new()` — ещё до того,
/// как нарисован хоть один кадр, — и из обработчика клавиш, когда терминал уже в raw-режиме.
/// Раньше тут стоял `Command::output()`, который ждёт выхода процесса и EOF на его потоках,
/// то есть буквально вечно: подвисший на сети `claude`/`codex` морозил Clave насмерть — без
/// экрана, без вывода, без объяснения. Переопределяется `CLAVE_AUTH_TIMEOUT_SECS`.
fn auth_timeout() -> Duration {
    auth_timeout_from(env::var("CLAVE_AUTH_TIMEOUT_SECS").ok().as_deref())
}

/// Разбор потолка из значения переменной. Мусор и ноль → дефолт: нулевой потолок убивал бы
/// пробу сразу после спавна, нечисловой — просто опечатка. Десяти секунд хватает даже
/// провайдеру, который лезет за токеном в сеть.
fn auth_timeout_from(value: Option<&str>) -> Duration {
    let secs = value
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&s| s > 0)
        .unwrap_or(10);
    Duration::from_secs(secs)
}

/// Статус немого провайдера. Обязан назвать причину: иначе в онбординге он выглядит как
/// «не залогинен», и пользователь идёт чинить логин, с которым всё в порядке.
fn auth_timeout_status(timeout: Duration) -> String {
    format!("no response in {}s", timeout.as_secs())
}

/// Проба логина с потолком по времени. Бинарь и потолок — ПАРАМЕТРЫ, а не окружение: иначе
/// «зависший CLI нас не морозит» проверялось бы только настоящим зависшим CLI, то есть никак.
fn auth_probe(binary: &str, args: &[&str], timeout: Duration) -> AuthProbe {
    let mut command = Command::new(binary);
    command
        .args(args)
        // Как делал `output()`: проба не смеет воровать ввод у TUI.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Лидерство в группе тут не украшение: `kill_process_tree` шлёт сигнал ГРУППЕ (`-pid`),
    // и без него сигнал не дошёл бы ни до кого, а `wait` внутри повис бы навсегда — починка
    // обернулась бы новым зависанием.
    configure_process_group(&mut command);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            return AuthProbe {
                installed: false,
                authenticated: false,
                status: err.to_string(),
            }
        }
    };

    // Потоки читаем в тредах: молчаливый CLI не забьёт пайп, а болтливый не упрётся в его
    // буфер (`output()` делал ровно это, только без потолка).
    let stdout = child.stdout.take().map(spawn_capture_reader);
    let stderr = child.stderr.take().map(spawn_capture_reader);
    let deadline = Instant::now() + timeout;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = stdout
                    .map(|h| h.join().unwrap_or_default())
                    .unwrap_or_default();
                let err = stderr
                    .map(|h| h.join().unwrap_or_default())
                    .unwrap_or_default();
                let text = command_output_text(out.as_bytes(), err.as_bytes());
                return AuthProbe {
                    installed: true,
                    authenticated: auth_output_looks_ready(status.success(), &text),
                    status: first_nonempty_line(&text)
                        .unwrap_or_else(|| "status unavailable".to_string()),
                };
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    // Убиваем группу целиком: внук, унаследовавший stdout, держал бы пайп
                    // открытым — тогда ридеры не дождались бы EOF, и join завис бы.
                    // Поэтому их не join-им, а роняем: их результат нам уже не нужен.
                    kill_process_tree(&mut child);
                    drop(stdout);
                    drop(stderr);
                    return AuthProbe {
                        installed: true,
                        authenticated: false,
                        status: auth_timeout_status(timeout),
                    };
                }
                thread::sleep(Duration::from_millis(20));
            }
            Err(err) => {
                kill_process_tree(&mut child);
                return AuthProbe {
                    installed: true,
                    authenticated: false,
                    status: err.to_string(),
                };
            }
        }
    }
}

/// Эвристика «залогинен ли провайдер» по выводу `*_auth_probe`. Стабильного контракта у
/// чужих CLI нет, поэтому по приоритету: явные маркеры НЕ-логина перекрывают всё (их
/// печатают и с exit 0), затем явный маркер логина (принимаем даже при ненулевом коде —
/// бывает диагностический вывод), иначе — доверяемся коду выхода пробы.
pub(crate) fn auth_output_looks_ready(success: bool, text: &str) -> bool {
    let lower = text.to_lowercase();

    if AUTH_NOT_READY_MARKERS.iter().any(|m| lower.contains(m)) {
        return false;
    }
    if AUTH_READY_MARKERS.iter().any(|m| lower.contains(m)) {
        return true;
    }
    success
}

const AUTH_NOT_READY_MARKERS: &[&str] = &[
    "not logged",
    "not authenticated",
    "not signed",
    "login required",
    "please log in",
    "please login",
    "logged out",
    "no credentials",
    "unauthenticated",
    "auth required",
];

const AUTH_READY_MARKERS: &[&str] = &[
    "logged in",
    "logged into",
    "signed in",
    "authenticated as",
    "account:",
];

pub(crate) fn command_output_text(stdout: &[u8], stderr: &[u8]) -> String {
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(stdout));
    if !stderr.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&String::from_utf8_lossy(stderr));
    }
    text
}

pub(crate) fn first_nonempty_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("WARNING:"))
        .map(ToString::to_string)
}

pub(crate) fn run_external_command(command: &ExternalCommand) -> AnyResult<i32> {
    // Живой блок инлайн (без alt-screen): просто отпускаем raw-режим и пишем под ним.
    disable_raw_mode()?;
    execute!(io::stdout(), crossterm::cursor::Show)?;

    println!();
    println!(
        "Clave: running {} {}",
        command.program,
        command.args.join(" ")
    );
    println!();

    let result = Command::new(command.program).args(command.args).status();
    let code = match result {
        Ok(status) => status.code().unwrap_or(1),
        Err(err) => {
            println!("Clave: failed to start command: {err}");
            1
        }
    };

    println!();
    println!("Clave: press Enter to return...");
    let mut wait = String::new();
    let _ = io::stdin().read_line(&mut wait);

    enable_raw_mode()?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn onboarding(
        codex_authenticated: bool,
        claude_authenticated: bool,
        codex_installed: bool,
        claude_installed: bool,
    ) -> Onboarding {
        Onboarding {
            step: OnboardingStep::Auth,
            provider_index: 0,
            setting_index: 0,
            codex_installed,
            claude_installed,
            codex_authenticated,
            claude_authenticated,
            codex_status: String::new(),
            claude_status: String::new(),
            message: String::new(),
        }
    }

    #[test]
    fn auth_requirements_ready_checks_only_needed_providers() {
        // Нужен только Codex: решает его логин, состояние Claude не важно.
        assert!(auth_requirements_ready(
            Mode::CodexOnly,
            &onboarding(true, false, true, true)
        ));
        assert!(!auth_requirements_ready(
            Mode::CodexOnly,
            &onboarding(false, true, true, true)
        ));

        // Нужен только Claude: зеркально.
        assert!(auth_requirements_ready(
            Mode::ClaudeOnly,
            &onboarding(false, true, true, true)
        ));
        assert!(!auth_requirements_ready(
            Mode::ClaudeOnly,
            &onboarding(true, false, true, true)
        ));

        // Нужны оба: половины логина не хватает.
        assert!(!auth_requirements_ready(
            Mode::ClaudeCodex,
            &onboarding(true, false, true, true)
        ));
        assert!(!auth_requirements_ready(
            Mode::ClaudeCodex,
            &onboarding(false, true, true, true)
        ));
        assert!(!auth_requirements_ready(
            Mode::ClaudeCodex,
            &onboarding(false, false, true, true)
        ));
        assert!(auth_requirements_ready(
            Mode::ClaudeCodex,
            &onboarding(true, true, true, true)
        ));
        assert!(!auth_requirements_ready(
            Mode::CodexClaude,
            &onboarding(true, false, true, true)
        ));
        assert!(auth_requirements_ready(
            Mode::CodexClaude,
            &onboarding(true, true, true, true)
        ));
    }

    #[test]
    fn missing_auth_text_lists_only_needed_and_unauthenticated() {
        // Всё нужное залогинено — сообщения о нехватке нет.
        assert_eq!(
            missing_auth_text(
                Mode::CodexOnly,
                &onboarding(true, false, true, true),
                Language::Ru
            ),
            "всё готово"
        );
        assert_eq!(
            missing_auth_text(
                Mode::CodexOnly,
                &onboarding(true, false, true, true),
                Language::En
            ),
            "all ready"
        );

        // Нужен Codex и не залогинен, Claude не нужен и тоже не залогинен — только Codex.
        assert_eq!(
            missing_auth_text(
                Mode::CodexOnly,
                &onboarding(false, false, true, true),
                Language::Ru
            ),
            "Codex"
        );
        // Зеркально: нужен только Claude.
        assert_eq!(
            missing_auth_text(
                Mode::ClaudeOnly,
                &onboarding(false, false, true, true),
                Language::Ru
            ),
            "Claude"
        );
        assert_eq!(
            missing_auth_text(
                Mode::ClaudeOnly,
                &onboarding(false, true, true, true),
                Language::Ru
            ),
            "всё готово"
        );

        // Оба нужны, оба не залогинены и не установлены — обе локализованные ветки и разделитель.
        assert_eq!(
            missing_auth_text(
                Mode::ClaudeCodex,
                &onboarding(false, false, false, false),
                Language::Ru
            ),
            "Codex CLI не найден + Claude CLI не найден"
        );
        assert_eq!(
            missing_auth_text(
                Mode::ClaudeCodex,
                &onboarding(false, false, false, false),
                Language::En
            ),
            "Codex CLI missing + Claude CLI missing"
        );
    }

    #[test]
    fn command_output_text_joins_streams_without_stray_newlines() {
        assert_eq!(command_output_text(b"out", b"err"), "out\nerr");
        assert_eq!(command_output_text(b"", b"err"), "err");
        assert_eq!(command_output_text(b"out", b""), "out");
        assert_eq!(command_output_text(b"", b""), "");
    }

    #[test]
    fn first_nonempty_line_skips_blanks_and_warnings() {
        assert_eq!(
            first_nonempty_line("   \n\n  WARNING: skip me\n  status: ok  \n"),
            Some("status: ok".to_string())
        );
        assert_eq!(first_nonempty_line("  \n\n"), None);
        assert_eq!(first_nonempty_line("WARNING: only"), None);
        assert_eq!(first_nonempty_line(""), None);
    }

    #[cfg(unix)]
    fn write_script(name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = env::temp_dir().join(format!("clave-auth-{}-{name}.sh", std::process::id()));
        fs::write(&path, body).expect("скрипт записан");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    /// Пробу гоняем в отдельном потоке и ждём с запасом. Смысл в том, что при откате
    /// починки тест ОБЯЗАН УПАСТЬ, а не повиснуть: набор, висящий вечно, ничего не
    /// сообщает — он просто вешает CI.
    #[cfg(unix)]
    fn probe_in_thread(binary: PathBuf, timeout: Duration) -> Receiver<AuthProbe> {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(auth_probe(&binary.to_string_lossy(), &[], timeout));
        });
        rx
    }

    // Провайдер замолчал — И оставил под собой внука, как делает настоящий CLI, поднимая свои
    // под-процессы. Один тест ловит сразу две вещи:
    //
    //   1. потолок срабатывает. Раньше `.output()` ждал бы вечно, а пробу зовут из `App::new()`
    //      ещё до первого кадра — подвисший CLI морозил всю Clave на старте: пустой экран и
    //      никаких объяснений.
    //
    //   2. убивается ВСЯ группа. Без `configure_process_group` процесс не лидер группы, сигнал
    //      `kill(-pid)` не дошёл бы ни до кого, и `wait()` внутри `kill_process_tree` завис бы
    //      на внуке (sh ждёт свой `sleep 30`). Проба вернулась бы через 30 с, а не через одну, —
    //      и `recv_timeout(15 с)` этот случай ловит.
    //
    // Гонки тут нет намеренно: тест ничего не выясняет про внука до срабатывания потолка.
    // Первая версия пыталась — читала pid внука из файла-маркера, пока проба ждёт, — и
    // разваливалась под нагрузкой (`sh` не успевал стартовать за отведённые секунды). Хуже
    // того: падая на ровном месте, она красила набор и заставляла cargo mutants записывать
    // мутантов в «пойманные», которых никто не ловил. Флейк не просто шумит — он ВРЁТ гейту,
    // и врёт в сторону «всё покрыто».
    #[cfg(unix)]
    #[test]
    fn a_mute_provider_with_a_grandchild_does_not_freeze_the_probe() {
        let script = write_script("mute", "#!/bin/sh\nsleep 30 &\nwait\n");
        let rx = probe_in_thread(script.clone(), Duration::from_secs(1));

        let probe = rx.recv_timeout(Duration::from_secs(15)).expect(
            "проба обязана вернуться по потолку — а вернуться она может, только если убита \
             ВСЯ группа: иначе wait() внутри kill_process_tree ждал бы внука полминуты",
        );
        let _ = fs::remove_file(&script);

        assert!(probe.installed, "бинарь запустился — значит, установлен");
        assert!(
            !probe.authenticated,
            "немой провайдер не может считаться залогиненным"
        );
        assert_eq!(
            probe.status, "no response in 1s",
            "статус обязан назвать причину: иначе это выглядит как «не залогинен», \
             и пользователь пойдёт чинить логин, с которым всё в порядке"
        );
    }

    #[cfg(unix)]
    #[test]
    fn probe_takes_the_answer_of_a_live_provider() {
        let script = write_script("ready", "#!/bin/sh\necho 'Logged in as user@example.com'\n");
        let probe = auth_probe(&script.to_string_lossy(), &[], Duration::from_secs(10));
        let _ = fs::remove_file(&script);

        assert!(probe.installed);
        assert!(
            probe.authenticated,
            "явный маркер логина обязан приниматься"
        );
        assert_eq!(probe.status, "Logged in as user@example.com");
    }

    // stderr и ненулевой код: провайдер ругается не в stdout, и потолок тут ни при чём —
    // ответ обязан дойти целиком.
    #[cfg(unix)]
    #[test]
    fn probe_takes_stderr_and_the_exit_code() {
        let script = write_script(
            "denied",
            "#!/bin/sh\necho 'You are not logged in.' >&2\nexit 1\n",
        );
        let probe = auth_probe(&script.to_string_lossy(), &[], Duration::from_secs(10));
        let _ = fs::remove_file(&script);

        assert!(probe.installed, "процесс запустился — бинарь на месте");
        assert!(!probe.authenticated);
        assert_eq!(probe.status, "You are not logged in.");
    }

    #[test]
    fn probe_reports_a_missing_binary() {
        let probe = auth_probe(
            "/nonexistent/clave-no-such-provider",
            &[],
            Duration::from_secs(1),
        );
        assert!(!probe.installed, "несуществующий бинарь — не установлен");
        assert!(!probe.authenticated);
        assert!(!probe.status.is_empty(), "причина обязана быть названа");
    }

    // Потолок, схлопнувшийся в ноль, — это не «строгая проверка», а поломка: пробу убивало бы
    // сразу после спавна, и КАЖДЫЙ провайдер выглядел бы немым. Ловушка не гипотетическая:
    // `Duration::default()` и есть ноль, и мутационный гейт показал, что без этого теста
    // такую подмену не замечает никто.
    #[test]
    fn auth_timeout_is_never_zero() {
        assert!(
            auth_timeout() > Duration::ZERO,
            "нулевой потолок объявил бы немым любого провайдера, даже здорового"
        );
    }

    #[test]
    fn auth_timeout_falls_back_on_zero_and_garbage() {
        assert_eq!(auth_timeout_from(Some("30")), Duration::from_secs(30));
        // Ноль убивал бы пробу сразу после спавна — провайдер не успел бы и слова сказать.
        assert_eq!(auth_timeout_from(Some("0")), Duration::from_secs(10));
        assert_eq!(auth_timeout_from(Some("-5")), Duration::from_secs(10));
        assert_eq!(auth_timeout_from(Some("вечность")), Duration::from_secs(10));
        assert_eq!(auth_timeout_from(None), Duration::from_secs(10));
    }

    #[test]
    fn auth_timeout_status_names_the_limit() {
        assert_eq!(
            auth_timeout_status(Duration::from_secs(7)),
            "no response in 7s"
        );
    }

    #[test]
    fn auth_ready_prefers_explicit_markers_over_exit_code() {
        // Явный не-логин перекрывает даже успешный код выхода.
        assert!(!auth_output_looks_ready(true, "You are not logged in."));
        assert!(!auth_output_looks_ready(true, "Login required"));
        // Явный логин принимается даже при ненулевом коде.
        assert!(auth_output_looks_ready(
            false,
            "Logged in as user@example.com"
        ));
        assert!(auth_output_looks_ready(false, "Authenticated as acme"));
        // Нейтральный вывод — доверяемся коду выхода пробы.
        assert!(auth_output_looks_ready(true, "status: ok"));
        assert!(!auth_output_looks_ready(false, "status: ok"));
        // «not authenticated» не должен приниматься маркером «authenticated as».
        assert!(!auth_output_looks_ready(true, "user is not authenticated"));
    }
}
