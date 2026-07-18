use crate::prelude::*;
use crate::*;

mod claude;
mod codex;
pub(crate) use claude::*;
pub(crate) use codex::*;

/// Команда запуска провайдера. Единственное место, где решается «claude или codex»:
/// у обоих раннеров (чат и тандем) сборка одинакова.
pub(crate) fn provider_command(
    provider: &str,
    effort: &str,
    prompt: &str,
    codex_out: &Path,
    access: RunAccess,
) -> Command {
    if provider == "claude" {
        let mut command = Command::new(claude_binary());
        command.args(claude_chat_args(effort, access, prompt));
        return command;
    }
    let mut command = Command::new(codex_binary());
    command.args([
        "exec",
        "--json",
        "-o",
        &codex_out.to_string_lossy(),
        "-c",
        &format!("model_reasoning_effort=\"{}\"", effort),
        "--skip-git-repo-check",
        "--ephemeral",
        "--color",
        "never",
        "-s",
        access.codex_sandbox(),
        prompt,
    ]);
    command
}

/// Ридер stdout под провайдера: claude отдаёт stream-json (токены ответа + tool_use),
/// codex — JSONL событий. Возвращаемая строка — сырьё для `provider_result`.
pub(crate) fn spawn_provider_reader<R: Read + Send + 'static>(
    provider: &str,
    out: R,
    tx: Sender<WorkerEvent>,
    lang: Language,
    last_activity: Arc<Mutex<Instant>>,
) -> thread::JoinHandle<String> {
    if provider == "claude" {
        spawn_claude_activity_reader(out, tx, lang, last_activity)
    } else {
        spawn_codex_activity_reader(out, tx, lang, last_activity)
    }
}

/// Ответ провайдера из уже собранных кусков: claude кладёт всё в stdout (result-событие),
/// codex — текст в файл `-o`, usage в JSONL stdout, ошибку сообщает только кодом выхода.
pub(crate) fn provider_result(
    provider: &str,
    stdout: &str,
    codex_text: String,
) -> (String, Option<RunUsage>, bool) {
    if provider == "claude" {
        let parsed = parse_claude_response(stdout);
        return (parsed.text, parsed.usage, parsed.is_error);
    }
    (codex_text, parse_codex_usage(stdout), false)
}

/// Код выхода прогона: claude может выйти с 0, пометив ответ `is_error` — такой прогон
/// НЕ успешен, иначе ошибка провайдера утекла бы в чат как нормальный ответ.
pub(crate) fn final_code(status: Option<i32>, is_error: bool) -> i32 {
    let code = status.unwrap_or(1);
    if is_error && code == 0 {
        return 1;
    }
    code
}

/// Аргументы запуска `claude` для прямого чата. Вынесено отдельно ради теста:
/// `--strict-mcp-config` гарантирует, что доступны РОВНО инструменты из
/// `access` — без MCP-серверов из глобального конфига пользователя (иначе
/// `--tools ""` не отключает MCP, и `needs-auth`-сервер может зависнуть в `-p`).
pub(crate) fn claude_chat_args<'a>(
    effort: &'a str,
    access: RunAccess,
    prompt: &'a str,
) -> Vec<&'a str> {
    vec![
        "-p",
        "--effort",
        effort,
        "--no-session-persistence",
        "--strict-mcp-config",
        "--tools",
        access.claude_tools(),
        "--permission-mode",
        access.claude_permission(),
        "--max-turns",
        "20",
        "--output-format",
        "stream-json",
        // Токен-стрим ответа: claude шлёт content_block_delta по мере генерации
        // (иначе текст приходит одним блоком в конце).
        "--include-partial-messages",
        "--verbose",
        prompt,
    ]
}

#[allow(clippy::too_many_arguments)]
/// Бинарь claude: env-override (моки/тесты) → дефолт `claude`. Один источник
/// для запуска И для auth-пробы, иначе проба игнорит override (см. провайдер-пробы).
pub(crate) fn claude_binary() -> String {
    env::var("CLAVE_CLAUDE").unwrap_or_else(|_| "claude".to_string())
}

/// Бинарь codex: env-override (моки/тесты) → дефолт `codex`.
pub(crate) fn codex_binary() -> String {
    env::var("CLAVE_CODEX").unwrap_or_else(|_| "codex".to_string())
}

/// Один вызов провайдера для тандема. `cancel_rx` по ссылке — чтобы переиспользовать
/// на серии шагов. None = отменён в процессе. Активность инструментов стримится в `tx`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_provider_once(
    provider: &'static str,
    effort: &str,
    prompt: &str,
    work_dir: &Path,
    access: RunAccess,
    lang: Language,
    tx: &Sender<WorkerEvent>,
    cancel_rx: &Receiver<()>,
) -> io::Result<Option<TandemStep>> {
    let codex_out = TempOut::new("tandem");
    let mut command = provider_command(provider, effort, prompt, codex_out.path(), access);

    configure_process_group(&mut command);
    let mut child = command
        .current_dir(work_dir)
        // stdin = /dev/null: агент получает промт из аргументов и НЕ должен делить
        // терминал UI (иначе он мог бы перехватывать ввод или сбрасывать raw-режим
        // терминала на выходе → UI «зависает» и не реагирует на клавиши).
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let last_activity = Arc::new(Mutex::new(Instant::now()));
    let stdout_handle = child
        .stdout
        .take()
        .map(|out| spawn_provider_reader(provider, out, tx.clone(), lang, last_activity.clone()));
    let stderr_handle = child.stderr.take().map(spawn_capture_reader);

    loop {
        if cancel_rx.try_recv().is_ok() {
            // Убиваем всю группу (CLI + под-процессы) и роняем ридеры: после смерти
            // группы пайпы закрываются, треды-ридеры завершатся сами по EOF. join здесь
            // делать НЕЛЬЗЯ — read мог бы зависнуть, держи пайп внук.
            kill_process_tree(&mut child);
            drop(stdout_handle);
            drop(stderr_handle);
            return Ok(None);
        }

        match child.try_wait()? {
            Some(status) => {
                let stdout = stdout_handle
                    .map(|handle| handle.join().unwrap_or_default())
                    .unwrap_or_default();
                let stderr = stderr_handle
                    .map(|handle| handle.join().unwrap_or_default())
                    .unwrap_or_default();

                let (mut text, usage, is_error) =
                    provider_result(provider, &stdout, codex_out.read());
                let code = final_code(status.code(), is_error);
                // Ошибка без ответа: причина — в stderr (или транзиент, если и он пуст). Раньше
                // stderr здесь ВЫБРАСЫВАЛСЯ (`let _ =`), фаза отдавала пустой текст, и пользователь
                // видел «код N» без единой строки причины. Чат-путь stderr сохраняет — тандем теперь тоже.
                if code != 0 && text.trim().is_empty() {
                    text = provider_error_detail_lines(&stderr, lang).join("\n");
                }
                return Ok(Some(TandemStep { text, code, usage }));
            }
            None => {
                if idle_expired(idle_elapsed(&last_activity), idle_timeout()) {
                    // Зависший CLI: убиваем всю его группу (CLI + под-процессы), чтобы
                    // закрылись пайпы, и роняем ридеры — они завершатся сами по EOF.
                    kill_process_tree(&mut child);
                    drop(stdout_handle);
                    drop(stderr_handle);
                    return Ok(Some(TandemStep {
                        text: idle_timeout_message(lang),
                        code: 124,
                        usage: None,
                    }));
                }
                thread::sleep(Duration::from_millis(80));
            }
        }
    }
}

pub(crate) fn short_path(path: &str) -> String {
    let tail: Vec<&str> = path.rsplit('/').take(2).collect();
    tail.into_iter().rev().collect::<Vec<_>>().join("/")
}

pub(crate) fn spawn_capture_reader<R>(reader: R) -> thread::JoinHandle<String>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        // read_to_end + lossy, а НЕ read_to_string: один невалидный UTF-8 байт заставил бы
        // read_to_string вернуть Err и не дописать прочитанное — весь вывод обнулился бы,
        // что может перевернуть вердикт авторизации. Лоссовое декодирование сохраняет текст.
        let mut reader = BufReader::new(reader);
        let mut bytes = Vec::new();
        let _ = reader.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_chat_args_are_strict_and_mode_scoped() {
        // --strict-mcp-config обязателен везде: иначе MCP-инструменты из
        // глобального конфига протекают мимо --tools.
        for access in [
            RunAccess::Chat(ChatMode::Discussion),
            RunAccess::Chat(ChatMode::Plan),
            RunAccess::PlanReadonly,
            RunAccess::PlanExecute,
        ] {
            let args = claude_chat_args("high", access, "hi");
            assert!(
                args.contains(&"--strict-mcp-config"),
                "strict-mcp-config missing for {access:?}"
            );
        }

        let discussion = claude_chat_args("high", RunAccess::Chat(ChatMode::Discussion), "hi");
        let tools_idx = discussion
            .iter()
            .position(|a| *a == "--tools")
            .expect("--tools present");
        assert_eq!(
            discussion[tools_idx + 1],
            "",
            "Discussion must be tool-free"
        );

        let readonly = claude_chat_args("high", RunAccess::PlanReadonly, "hi");
        let ro_tools = readonly
            .iter()
            .position(|a| *a == "--tools")
            .expect("--tools present");
        assert!(readonly[ro_tools + 1].contains("Read"));
        assert!(!readonly[ro_tools + 1].contains("Bash"));

        let execute = claude_chat_args("high", RunAccess::PlanExecute, "hi");
        let ex_tools = execute
            .iter()
            .position(|a| *a == "--tools")
            .expect("--tools present");
        assert!(execute[ex_tools + 1].contains("Bash"));
    }

    #[test]
    fn provider_command_is_built_per_provider() {
        let codex_out = PathBuf::from("/tmp/clave-out.txt");
        let args = |command: &Command| -> Vec<String> {
            command
                .get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect()
        };

        let claude = provider_command("claude", "max", "промт", &codex_out, RunAccess::PlanExecute);
        let claude_args = args(&claude);
        assert!(claude_args.contains(&"-p".to_string()), "{claude_args:?}");
        assert!(!claude_args.contains(&"exec".to_string()), "не codex-вызов");
        assert_eq!(claude_args.last().map(String::as_str), Some("промт"));

        let codex = provider_command(
            "codex",
            "xhigh",
            "промт",
            &codex_out,
            RunAccess::PlanReadonly,
        );
        let codex_args = args(&codex);
        assert!(codex_args.contains(&"exec".to_string()), "{codex_args:?}");
        assert!(!codex_args.contains(&"-p".to_string()), "не claude-вызов");
        // Вывод codex забираем файлом, песочница — из прав запуска, effort — в -c.
        let out_idx = codex_args
            .iter()
            .position(|a| a == "-o")
            .expect("-o присутствует");
        assert_eq!(codex_args[out_idx + 1], "/tmp/clave-out.txt");
        let sandbox_idx = codex_args
            .iter()
            .position(|a| a == "-s")
            .expect("-s присутствует");
        assert_eq!(
            codex_args[sandbox_idx + 1],
            RunAccess::PlanReadonly.codex_sandbox()
        );
        assert!(codex_args
            .iter()
            .any(|a| a == "model_reasoning_effort=\"xhigh\""));
        assert_eq!(codex_args.last().map(String::as_str), Some("промт"));
    }

    #[test]
    fn provider_reader_is_chosen_by_provider() {
        let (tx, _rx) = mpsc::channel();
        let last = Arc::new(Mutex::new(Instant::now()));
        let event = "{\"type\":\"turn.completed\",\"usage\":{}}\n";

        // codex-ридер отдаёт ВЕСЬ stdout (в нём usage), claude-ридер — только result.
        let codex = spawn_provider_reader(
            "codex",
            io::Cursor::new(event),
            tx.clone(),
            Language::Ru,
            last.clone(),
        );
        assert_eq!(codex.join().expect("ридер завершился"), event);

        let claude = spawn_provider_reader(
            "claude",
            io::Cursor::new(event),
            tx.clone(),
            Language::Ru,
            last.clone(),
        );
        assert_eq!(claude.join().expect("ридер завершился"), "");

        let result_line = "{\"type\":\"result\",\"result\":\"ок\"}";
        let claude = spawn_provider_reader(
            "claude",
            io::Cursor::new(result_line),
            tx,
            Language::Ru,
            last,
        );
        assert_eq!(claude.join().expect("ридер завершился"), result_line);
    }

    #[test]
    fn provider_result_takes_text_from_the_right_source() {
        // claude: всё в stdout — файл codex игнорируем, is_error поднимаем.
        let stdout = r#"{"type":"result","is_error":true,"result":"боом","usage":{"input_tokens":7,"output_tokens":3}}"#;
        let (text, usage, is_error) = provider_result("claude", stdout, "мусор из файла".into());
        assert_eq!(text, "боом");
        assert!(is_error);
        assert_eq!(usage.expect("usage claude").input, 7);

        // codex: текст — из файла `-o`, usage — из JSONL, ошибок в выводе нет.
        let jsonl = "{\"usage\":{\"input_tokens\":11,\"output_tokens\":2}}\n";
        let (text, usage, is_error) = provider_result("codex", jsonl, "ответ codex".into());
        assert_eq!(text, "ответ codex");
        assert_eq!(usage.expect("usage codex").input, 11);
        assert!(!is_error, "codex сообщает об ошибке только кодом выхода");
    }

    #[test]
    fn final_code_fails_run_on_soft_claude_error() {
        assert_eq!(final_code(Some(0), false), 0);
        // claude вышел с 0, но пометил ответ ошибкой — прогон НЕ успешен.
        assert_eq!(final_code(Some(0), true), 1);
        // код провайдера не переписываем.
        assert_eq!(final_code(Some(3), true), 3);
        assert_eq!(final_code(Some(3), false), 3);
        // убит сигналом (status.code() == None) — тоже провал.
        assert_eq!(final_code(None, false), 1);
    }

    #[test]
    fn provider_binaries_default_to_cli_names() {
        // Пробы авторизации и запуск обязаны брать один и тот же бинарь.
        assert_eq!(claude_binary(), "claude");
        assert_eq!(codex_binary(), "codex");
    }

    // --- чистая логика чата ---

    #[test]
    fn capture_reader_keeps_output_despite_invalid_utf8() {
        // Один невалидный UTF-8 байт (0xFF) между валидными: read_to_string вернул бы Err
        // и обнулил ВЕСЬ вывод (мог перевернуть вердикт авторизации). Лоссовое чтение спасает.
        let data = b"ok\xFFmore".to_vec();
        let out = spawn_capture_reader(std::io::Cursor::new(data))
            .join()
            .expect("ридер завершился");
        assert!(
            out.contains("ok") && out.contains("more"),
            "текст вокруг битого байта не потерян: {out:?}"
        );
    }

    // Единственный тест, который реально запускает «провайдера»: жизненный цикл процесса
    // (спавн → чтение stdout → код выхода → разбор ответа) нечем проверить иначе. Бинарь
    // подменяем штатным env-override (он и заведён для моков).
    //
    // Но `CLAVE_CLAUDE` — глобальная на процесс переменная, и `claude_binary()` читают многие
    // (auth-проба, `App::new`, `provider_binaries_default_to_cli_names`). Выставь её здесь через
    // `set_var` — и параллельный читатель в том же `cargo test` поймает путь к скрипту вместо
    // дефолта «claude». Ровно эта гонка мигала под нагрузкой. Поэтому — как в `ui::footer` — тест
    // перезапускает СЕБЯ дочерним процессом, где `CLAVE_CLAUDE` задана снаружи только ему; env
    // самого `cargo test` не трогаем ни на миг.
    #[cfg(unix)]
    #[test]
    fn run_provider_once_runs_the_cli_and_maps_its_answer() {
        use std::os::unix::fs::PermissionsExt;

        // Дочерний процесс: `CLAVE_CLAUDE` уже указывает на фейковый бинарь — просто гоняем.
        if env::var(PROVIDER_CHILD).is_ok() {
            let (tx, _rx) = mpsc::channel();
            let (_cancel_tx, cancel_rx) = mpsc::channel();
            let outcome = run_provider_once(
                "claude",
                "max",
                "промт",
                &env::temp_dir(),
                RunAccess::PlanReadonly,
                Language::Ru,
                &tx,
                &cancel_rx,
            );

            let step = outcome
                .expect("провайдер запустился")
                .expect("шаг не отменён");
            assert_eq!(step.text, "готово");
            assert_eq!(step.code, 0);
            assert_eq!(step.usage.expect("usage разобран").input, 5);
            return;
        }

        // Родитель: создаём фейковый бинарь и запускаем себя ребёнком с `CLAVE_CLAUDE` только у него.
        let script = private_temp_dir().join("fake-claude.sh");
        fs::write(
            &script,
            "#!/bin/sh\necho '{\"type\":\"result\",\"is_error\":false,\"result\":\"готово\",\
             \"usage\":{\"input_tokens\":5,\"output_tokens\":2}}'\n",
        )
        .expect("скрипт записан");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");

        let exe = env::current_exe().expect("путь к тестовому бинарю");
        let out = Command::new(exe)
            .args([PROVIDER_SELF, "--exact", "--nocapture"])
            .env(PROVIDER_CHILD, "1")
            .env("CLAVE_CLAUDE", &script)
            .output()
            .expect("дочерний тест не запустился");
        let _ = fs::remove_file(&script);

        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "дочерний прогон провалился:\n{stdout}{}",
            String::from_utf8_lossy(&out.stderr),
        );
        // Коду возврата тут верить нельзя: с опечаткой в фильтре ребёнок гоняет НОЛЬ тестов и
        // выходит нулём («0 passed; N filtered out») — успех из ничего. Требуем предъявить прогнанное.
        assert!(
            stdout.contains("1 passed"),
            "дочерний прогон не состоялся: фильтр не нашёл тест, а ноль тестов читаются как успех. \
             Вывод:\n{stdout}"
        );
    }

    /// Родитель говорит ребёнку взять свою ветку этого теста (и не крутить всё заново).
    const PROVIDER_CHILD: &str = "CLAVE_TEST_RUN_PROVIDER_CHILD";
    /// Полный путь теста — им фильтруется дочерний прогон.
    const PROVIDER_SELF: &str =
        "worker::provider::tests::run_provider_once_runs_the_cli_and_maps_its_answer";
}
