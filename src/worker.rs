use crate::prelude::*;
use crate::*;

/// Настраивает дочерний процесс лидером НОВОЙ группы процессов, чтобы его собственные
/// под-процессы (Bash-инструменты агента) попадали в ту же группу. Тогда при отмене или
/// таймауте всё дерево убивается разом (см. `kill_process_tree`), а не только сам CLI.
/// На не-unix — no-op (там дерево завершает `child.kill()`).
#[cfg(unix)]
pub(crate) fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
pub(crate) fn configure_process_group(_command: &mut Command) {}

/// Убивает дочерний процесс ВМЕСТЕ со всей его группой (внуками) и пожинает зомби.
/// На unix шлём SIGKILL всей группе через отрицательный pid: процесс спавнился лидером
/// группы (`configure_process_group`), значит pgid == его pid, и цель сигнала — `-pid`.
/// Это закрывает stdout/stderr-пайпы, которые мог держать под-процесс агента, — иначе
/// тред-ридер завис бы на `read` навсегда.
pub(crate) fn kill_process_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        // SAFETY: kill(2) с отрицательным pid адресует группу процессов; аргументы —
        // простые скаляры, предусловий на память нет.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
    let _ = child.wait();
}

/// Спавнит рабочий поток, гарантируя терминальное событие даже при панике его тела.
/// Без этого паника воркера оставила бы `running=true` и вечный лоадер (главный поток
/// продолжает жить). Панику ловим (её вывод подавлен в panic-hook) и шлём Failed.
pub(crate) fn spawn_worker<F>(fail_tx: Sender<WorkerEvent>, body: F)
where
    F: FnOnce() + Send + 'static,
{
    thread::spawn(move || {
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)).is_err() {
            let _ = fail_tx.send(WorkerEvent::Failed(
                "внутренняя ошибка Clave (паника рабочего потока)".to_string(),
            ));
        }
    });
}

/// Приватный подкаталог для временных файлов (0700 на unix): вывод codex кладём туда,
/// чтобы на многопользовательской машине его нельзя было подменить симлинком до записи —
/// каталог принадлежит нам, посторонний в него не запишет. Если создать не удалось,
/// падаем на общий temp_dir (лучше работать, чем упасть).
fn private_temp_dir() -> PathBuf {
    let dir = env::temp_dir().join(format!("clave-{}", std::process::id()));
    if fs::create_dir_all(&dir).is_err() {
        return env::temp_dir();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    }
    dir
}

pub(crate) fn spawn_reader<R>(reader: R, tx: Sender<WorkerEvent>)
where
    R: io::Read + Send + 'static,
{
    thread::spawn(move || {
        let reader = BufReader::new(reader);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    let _ = tx.send(WorkerEvent::Line(line));
                }
                Err(err) => {
                    let _ = tx.send(WorkerEvent::Line(format!("read error: {err}")));
                    break;
                }
            }
        }
    });
}

pub(crate) fn estimate_tokens(text: &str) -> usize {
    let chars = text.chars().count();
    let words = text.split_whitespace().count();
    ((chars / 4).max(words)).max(1)
}

pub(crate) fn format_token_count(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}m", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

pub(crate) fn provider_display(provider: &str, lang: Language) -> &'static str {
    match provider {
        "codex" => "Codex",
        "claude" => "Claude",
        _ => lang.choose("Модель", "Model"),
    }
}

pub(crate) fn chat_prompt(message: &str, context: &str, lang: Language, mode: ChatMode) -> String {
    let language_hint = lang.choose(
        "Отвечай на русском, если пользователь не просит другой язык.",
        "Reply in English unless the user asks for another language.",
    );
    let mode_hint = mode.prompt_hint(lang);
    let ask_hint = lang.choose(
        "Если для продолжения нужен выбор пользователя, можешь в САМОМ КОНЦЕ ответа вывести \
         ровно один блок ```clave-ask с JSON: {\"question\":\"...\",\"multi\":false,\
         \"options\":[{\"label\":\"...\",\"note\":\"...\"}]}. Минимум 2 варианта, label кратко, \
         note — необязательная подсказка, multi=true если можно выбрать несколько. Можно \
         задать несколько вопросов сразу: {\"questions\":[{...},{...}]} (до 4). Блок — \
         последнее в ответе; после него ничего не пиши и не отвечай за пользователя. \
         Используй редко — только когда выбор действительно нужен.",
        "If you need the user to choose before continuing, you MAY end your answer with \
         exactly one ```clave-ask block of JSON: {\"question\":\"...\",\"multi\":false,\
         \"options\":[{\"label\":\"...\",\"note\":\"...\"}]}. At least 2 options, short labels, \
         optional note, multi=true to allow several. You may ask several questions at once: \
         {\"questions\":[{...},{...}]} (up to 4). The block must be the very last thing — \
         write nothing after it and do not answer for the user. Use sparingly, only when a \
         choice is genuinely needed.",
    );
    format!(
        "You are {APP_NAME}, an AI assistant inside a terminal UI.\n\
         {mode_hint}\n\
         Keep your final answer concise and useful. {language_hint}\n\n\
         {ask_hint}\n\n\
         Recent chat context:\n{context}\n\n\
         User message:\n{message}",
        mode_hint = mode_hint,
        language_hint = language_hint,
        ask_hint = ask_hint,
        context = if context.trim().is_empty() {
            "(empty)"
        } else {
            context
        },
        message = message
    )
}

pub(crate) fn recent_chat_context(transcript: &[String], max_lines: usize) -> String {
    transcript
        .iter()
        .rev()
        .filter(|line| !line.starts_with("⏺ Отправляю") && !line.starts_with("⏺ Sending"))
        .take(max_lines)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|line| truncate_chars(line, 240))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn plan_prompt(task: &str, context: &str, lang: Language) -> String {
    let language_hint = lang.choose(
        "Отвечай на русском, если пользователь не просит другой язык.",
        "Reply in English unless the user asks for another language.",
    );
    format!(
        "You are {APP_NAME}, an AI assistant inside a terminal UI, in PLAN MODE.\n\
         Study the working directory (read files, search) and produce a concrete, \
         step-by-step implementation plan for the task. For each step name the files \
         to touch and what changes it makes; list risks or open questions at the end.\n\
         Do NOT modify any files and do NOT run shell commands — planning only.\n\
         {language_hint}\n\n\
         Recent chat context:\n{context}\n\n\
         Task:\n{task}",
        language_hint = language_hint,
        context = if context.trim().is_empty() {
            "(empty)"
        } else {
            context
        },
        task = task,
    )
}

pub(crate) fn execute_prompt(task: &str, plan: &str, context: &str, lang: Language) -> String {
    let language_hint = lang.choose(
        "Отвечай на русском, если пользователь не просит другой язык.",
        "Reply in English unless the user asks for another language.",
    );
    format!(
        "You are {APP_NAME}, an AI assistant inside a terminal UI, executing an APPROVED plan.\n\
         Implement the task fully: read, create and edit files and run commands in the \
         working directory as needed. Follow the plan; if reality differs, adapt but stay \
         within its intent. Keep your final answer concise and useful. {language_hint}\n\n\
         Recent chat context:\n{context}\n\n\
         Task:\n{task}\n\n\
         Approved plan:\n{plan}",
        language_hint = language_hint,
        context = if context.trim().is_empty() {
            "(empty)"
        } else {
            context
        },
        task = task,
        plan = plan,
    )
}

pub(crate) fn refine_prompt(
    task: &str,
    prev_plan: &str,
    feedback: &str,
    context: &str,
    lang: Language,
) -> String {
    let language_hint = lang.choose(
        "Отвечай на русском, если пользователь не просит другой язык.",
        "Reply in English unless the user asks for another language.",
    );
    format!(
        "You are {APP_NAME}, an AI assistant inside a terminal UI, in PLAN MODE.\n\
         Revise the previous plan to address the user's feedback. Same rules: read-only — \
         Do NOT modify files or run commands; numbered steps with files to touch and \
         risks at the end. {language_hint}\n\n\
         Recent chat context:\n{context}\n\n\
         Task:\n{task}\n\n\
         Previous plan:\n{prev_plan}\n\n\
         User feedback to address:\n{feedback}",
        language_hint = language_hint,
        context = if context.trim().is_empty() {
            "(empty)"
        } else {
            context
        },
        task = task,
        prev_plan = prev_plan,
        feedback = feedback,
    )
}

/// Ищет строку-СИГНАЛ (строго `TANDEM: CONSENSUS` / `TANDEM: CONTINUE`, снизу вверх):
/// CONSENSUS → true. Дефолт false (= CONTINUE) — безопаснее продолжить, чем ложно
/// согласиться (P1). Строгий разбор не даёт упоминанию `TANDEM:` в прозе завершить дебаты.
pub(crate) fn parse_tandem_signal(text: &str) -> bool {
    for line in text.lines().rev() {
        // Считаем строку сигналом, только если после снятия markdown-обрамления она
        // НАЧИНАЕТСЯ с `TANDEM:`, а остаток — ровно CONSENSUS или CONTINUE. Иначе фраза
        // из рассуждений вроде «...output TANDEM: CONSENSUS when done» дала бы ложный
        // консенсус и преждевременно завершила бы дебаты.
        let cleaned = line
            .trim()
            .trim_matches(|c: char| c == '*' || c == '`' || c == '>' || c == ' ');
        let upper = cleaned.to_uppercase();
        let Some(rest) = upper.strip_prefix("TANDEM:") else {
            continue;
        };
        return rest.trim() == "CONSENSUS";
    }
    false
}

fn tandem_lang_hint(lang: Language) -> &'static str {
    lang.choose(
        "Отвечай на русском, если пользователь не просит другой язык.",
        "Reply in English unless the user asks for another language.",
    )
}

/// Общий инженерный кодекс для tandem-промптов (обе роли).
const ENGINEERING_PRINCIPLES: &str = "Engineering principles (every step):\n\
- Simplicity first: prefer the smallest solution that fully solves the task. Complexity and abstraction must earn their place.\n\
- Stay lean on dependencies: default to the standard library and crates/packages already in this project. Do NOT add a new or heavy dependency unless clearly justified — if you do, state in one line what it buys and why existing code won't do.\n\
- Respect THIS codebase: follow its existing architecture, conventions and patterns; read the relevant files before proposing. Don't graft in foreign paradigms.\n\
- YAGNI: build what the task needs now, not speculative generality.";

/// Добавка для роли критика: судить, а не поддакивать.
const CRITIC_DISCIPLINE: &str = "Judge, don't rubber-stamp: do NOT trust the other agent's answer at face value. Read what was actually proposed/changed, verify it against the real code, and form your OWN reasoned opinion. Agree only after checking; when you disagree, be specific. Actively flag any unjustified dependency, heavy library, or added complexity, and name the leaner alternative.";

/// Добавка для роли исполнителя: минимализм.
const EXECUTOR_DISCIPLINE: &str = "Reach for the minimal change first; if you add a dependency or abstraction, justify it in one line.";

pub(crate) fn tandem_propose_prompt(task: &str, transcript: &str, lang: Language) -> String {
    format!(
        "You are {APP_NAME}, the EXECUTOR working in a pair with a CRITIC. PLAN MODE.\n\
         Study the working directory (read files, search). Propose a concrete approach to \
         the task: which files, what changes, and why. Address the critic's prior objections \
         if any. Do NOT modify files or run commands — this is discussion.\n\n\
         {principles}\n\n{discipline}\n\n\
         {hint}\n\n\
         Task:\n{task}\n\n\
         Tandem transcript so far:\n{transcript}",
        principles = ENGINEERING_PRINCIPLES,
        discipline = EXECUTOR_DISCIPLINE,
        hint = tandem_lang_hint(lang),
        task = task,
        transcript = if transcript.trim().is_empty() {
            "(empty)"
        } else {
            transcript
        },
    )
}

pub(crate) fn tandem_challenge_prompt(task: &str, transcript: &str, lang: Language) -> String {
    format!(
        "You are {APP_NAME}, the CRITIC working in a pair with an EXECUTOR. PLAN MODE.\n\
         Study the code (read-only) and STRICTLY evaluate the executor's proposed approach: \
         gaps, risks, what is missing, better alternatives. Do NOT agree out of politeness. \
         End with EXACTLY one line: `TANDEM: CONSENSUS` only if the approach is genuinely \
         correct and complete, otherwise `TANDEM: CONTINUE` followed by concrete objections.\n\n\
         {principles}\n\n{discipline}\n\n\
         {hint}\n\n\
         Task:\n{task}\n\n\
         Tandem transcript so far:\n{transcript}",
        principles = ENGINEERING_PRINCIPLES,
        discipline = CRITIC_DISCIPLINE,
        hint = tandem_lang_hint(lang),
        task = task,
        transcript = if transcript.trim().is_empty() {
            "(empty)"
        } else {
            transcript
        },
    )
}

pub(crate) fn tandem_execute_prompt(task: &str, transcript: &str, lang: Language) -> String {
    format!(
        "You are {APP_NAME}, the EXECUTOR. The approach below was agreed with the critic. \
         Implement the task fully in the working directory: read, create and edit files and \
         run commands as needed. If reality differs from the plan, adapt within its intent. \
         Keep your final answer concise.\n\n\
         {principles}\n\n{discipline}\n\n\
         {hint}\n\n\
         Task:\n{task}\n\n\
         Agreed approach / transcript:\n{transcript}",
        principles = ENGINEERING_PRINCIPLES,
        discipline = EXECUTOR_DISCIPLINE,
        hint = tandem_lang_hint(lang),
        task = task,
        transcript = if transcript.trim().is_empty() {
            "(empty)"
        } else {
            transcript
        },
    )
}

pub(crate) fn tandem_review_prompt(task: &str, transcript: &str, lang: Language) -> String {
    format!(
        "You are {APP_NAME}, the CRITIC. The executor applied the approach. Inspect the REAL \
         result (read the changed files). Does it match what was agreed, is it correct, any \
         bugs or omissions? End with EXACTLY one line: `TANDEM: CONSENSUS` if the result is \
         good, otherwise `TANDEM: CONTINUE` followed by what to fix.\n\n\
         {principles}\n\n{discipline}\n\n\
         {hint}\n\n\
         Task:\n{task}\n\n\
         Tandem transcript so far:\n{transcript}",
        principles = ENGINEERING_PRINCIPLES,
        discipline = CRITIC_DISCIPLINE,
        hint = tandem_lang_hint(lang),
        task = task,
        transcript = if transcript.trim().is_empty() {
            "(empty)"
        } else {
            transcript
        },
    )
}

pub(crate) fn tandem_fix_prompt(
    task: &str,
    transcript: &str,
    review: &str,
    lang: Language,
) -> String {
    format!(
        "You are {APP_NAME}, the EXECUTOR. The critic raised issues with the result. Fix them \
         in the working directory. Keep your final answer concise.\n\n\
         {principles}\n\n{discipline}\n\n\
         {hint}\n\n\
         Task:\n{task}\n\n\
         Critic's review to address:\n{review}\n\n\
         Tandem transcript so far:\n{transcript}",
        principles = ENGINEERING_PRINCIPLES,
        discipline = EXECUTOR_DISCIPLINE,
        hint = tandem_lang_hint(lang),
        task = task,
        review = review,
        transcript = if transcript.trim().is_empty() {
            "(empty)"
        } else {
            transcript
        },
    )
}

pub(crate) fn tandem_confirm_prompt(task: &str, transcript: &str, lang: Language) -> String {
    format!(
        "You are {APP_NAME}, the CRITIC. The executor applied fixes. Briefly verify whether \
         your issues are resolved (read the changed files). End with EXACTLY one line: \
         `TANDEM: CONSENSUS` if resolved, otherwise `TANDEM: CONTINUE` with what remains.\n\n\
         {principles}\n\n{discipline}\n\n\
         {hint}\n\n\
         Task:\n{task}\n\n\
         Tandem transcript so far:\n{transcript}",
        principles = ENGINEERING_PRINCIPLES,
        discipline = CRITIC_DISCIPLINE,
        hint = tandem_lang_hint(lang),
        task = task,
        transcript = if transcript.trim().is_empty() {
            "(empty)"
        } else {
            transcript
        },
    )
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

/// Лимит простоя: нет вывода дольше него → провайдер считается зависшим и
/// убивается (иначе застрявший CLI висит до ручного Ctrl+C). Это «тишина», а не
/// общий таймаут — нормальный агентский прогон постоянно стримит события, поэтому
/// долгие, но активные раны не страдают. Переопределяется `CLAVE_IDLE_TIMEOUT_SECS`.
pub(crate) fn idle_timeout() -> Duration {
    let secs = env::var("CLAVE_IDLE_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&s| s > 0)
        .unwrap_or(180);
    Duration::from_secs(secs)
}

/// Отметить «была активность сейчас» (ридеры зовут на каждой строке вывода).
fn touch_activity(last: &Arc<Mutex<Instant>>) {
    if let Ok(mut guard) = last.lock() {
        *guard = Instant::now();
    }
}

#[allow(clippy::too_many_arguments)] // самостоятельный раннер провайдера; группировать незачем
pub(crate) fn run_chat_provider(
    provider: &'static str,
    effort: &str,
    prompt: &str,
    work_dir: &Path,
    cancel_rx: Receiver<()>,
    tx: Sender<WorkerEvent>,
    lang: Language,
    access: RunAccess,
) -> io::Result<ChatRunResult> {
    let result = run_chat_attempt(
        provider, effort, prompt, work_dir, &cancel_rx, &tx, lang, access,
    )?;
    if !is_transient_chat_failure(&result) {
        return Ok(result);
    }
    // Один ретрай: мгновенный exit≠0 без вывода и без stderr — обычно транзиент
    // (сеть / лимит / обрыв до result). Отмена и таймаут (124) НЕ ретраятся.
    if cancel_rx.try_recv().is_ok() {
        return Ok(result);
    }
    let _ = tx.send(WorkerEvent::Line(
        lang.choose(
            "⎿ транзиентный сбой — повтор один раз…",
            "⎿ transient failure — retrying once…",
        )
        .to_string(),
    ));
    run_chat_attempt(
        provider, effort, prompt, work_dir, &cancel_rx, &tx, lang, access,
    )
}

/// Транзиентный сбой, который имеет смысл повторить: процесс мгновенно вышел с
/// ненулевым кодом, не дав ни ответа, ни stderr. Таймаут (124) и отмена — не сюда.
fn is_transient_chat_failure(result: &ChatRunResult) -> bool {
    matches!(
        result,
        ChatRunResult::Completed(code, text, stderr, _)
            if *code != 0 && *code != 124 && text.trim().is_empty() && stderr.trim().is_empty()
    )
}

// Связный набор параметров запуска провайдера; группировка ради порога lint не улучшит.
#[allow(clippy::too_many_arguments)]
fn run_chat_attempt(
    provider: &'static str,
    effort: &str,
    prompt: &str,
    work_dir: &Path,
    cancel_rx: &Receiver<()>,
    tx: &Sender<WorkerEvent>,
    lang: Language,
    access: RunAccess,
) -> io::Result<ChatRunResult> {
    let codex_out_file = private_temp_dir().join(format!(
        "codex-{}.txt",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let mut command = if provider == "claude" {
        let mut command = Command::new(claude_binary());
        command.args(claude_chat_args(effort, access, prompt));
        command
    } else {
        let mut command = Command::new(codex_binary());
        let codex_out = codex_out_file.to_string_lossy().into_owned();
        command.args([
            "exec",
            "--json",
            "-o",
            &codex_out,
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
    };

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
    let stdout_handle = child.stdout.take().map(|out| {
        if provider == "claude" {
            spawn_claude_activity_reader(out, tx.clone(), lang, last_activity.clone())
        } else {
            spawn_codex_activity_reader(out, tx.clone(), lang, last_activity.clone())
        }
    });
    let stderr_handle = child.stderr.take().map(spawn_capture_reader);

    loop {
        if cancel_rx.try_recv().is_ok() {
            // Убиваем всю группу (CLI + его под-процессы) и роняем ридеры: после смерти
            // группы пайпы закрываются, треды-ридеры завершатся сами по EOF. join здесь
            // делать НЕЛЬЗЯ — read мог бы зависнуть, держи пайп внук.
            kill_process_tree(&mut child);
            drop(stdout_handle);
            drop(stderr_handle);
            let _ = fs::remove_file(&codex_out_file);
            return Ok(ChatRunResult::Cancelled);
        }

        match child.try_wait()? {
            Some(status) => {
                let stdout = stdout_handle
                    .map(|handle| handle.join().unwrap_or_default())
                    .unwrap_or_default();
                let stderr = stderr_handle
                    .map(|handle| handle.join().unwrap_or_default())
                    .unwrap_or_default();

                let (text, usage, is_error) = if provider == "claude" {
                    let parsed = parse_claude_response(&stdout);
                    (parsed.text, parsed.usage, parsed.is_error)
                } else {
                    let text = fs::read_to_string(&codex_out_file).unwrap_or_default();
                    let usage = parse_codex_usage(&stdout);
                    let _ = fs::remove_file(&codex_out_file);
                    (text, usage, false)
                };

                let mut code = status.code().unwrap_or(1);
                if is_error && code == 0 {
                    code = 1;
                }
                return Ok(ChatRunResult::Completed(code, text, stderr, usage));
            }
            None => {
                if last_activity
                    .lock()
                    .map(|t| t.elapsed())
                    .unwrap_or_default()
                    >= idle_timeout()
                {
                    // Зависший CLI: убиваем всю его группу (CLI + под-процессы), чтобы
                    // закрылись пайпы, и роняем ридеры — они завершатся сами по EOF.
                    kill_process_tree(&mut child);
                    drop(stdout_handle);
                    drop(stderr_handle);
                    let _ = fs::remove_file(&codex_out_file);
                    let secs = idle_timeout().as_secs();
                    let msg = format!(
                        "{} {secs}{}",
                        lang.choose("Провайдер не отвечал", "Provider produced no output for"),
                        lang.choose(" c — остановлен по таймауту.", "s — stopped (timeout)."),
                    );
                    return Ok(ChatRunResult::Completed(124, String::new(), msg, None));
                }
                thread::sleep(Duration::from_millis(80));
            }
        }
    }
}

pub(crate) struct TandemStep {
    pub(crate) text: String,
    pub(crate) code: i32,
    pub(crate) usage: Option<RunUsage>,
}

pub(crate) enum TandemResult {
    Completed(i32, Option<RunUsage>),
    Cancelled,
}

/// Лента тандема, передаётся целиком в каждый промпт (P6: усечение при росте).
struct TandemTranscript {
    entries: Vec<String>,
}

impl TandemTranscript {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn push(&mut self, who: &str, phase: &str, text: &str) {
        self.entries
            .push(format!("[{who} · {phase}]\n{}", text.trim()));
    }

    fn render(&self) -> String {
        let full = self.entries.join("\n\n");
        if full.len() <= 12_000 || self.entries.len() <= 4 {
            return full;
        }
        // P6: оставляем первую запись + хвост (последние 3)
        let head = &self.entries[0];
        let tail = &self.entries[self.entries.len() - 3..];
        format!(
            "{head}\n\n…[ранние раунды усечены]…\n\n{}",
            tail.join("\n\n")
        )
    }
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
    let codex_out_file = private_temp_dir().join(format!(
        "tandem-{}.txt",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let mut command = if provider == "claude" {
        let mut command = Command::new(claude_binary());
        command.args(claude_chat_args(effort, access, prompt));
        command
    } else {
        let mut command = Command::new(codex_binary());
        let codex_out = codex_out_file.to_string_lossy().into_owned();
        command.args([
            "exec",
            "--json",
            "-o",
            &codex_out,
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
    };

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
    let stdout_handle = child.stdout.take().map(|out| {
        if provider == "claude" {
            spawn_claude_activity_reader(out, tx.clone(), lang, last_activity.clone())
        } else {
            spawn_codex_activity_reader(out, tx.clone(), lang, last_activity.clone())
        }
    });
    let stderr_handle = child.stderr.take().map(spawn_capture_reader);

    loop {
        if cancel_rx.try_recv().is_ok() {
            // Убиваем всю группу (CLI + под-процессы) и роняем ридеры: после смерти
            // группы пайпы закрываются, треды-ридеры завершатся сами по EOF. join здесь
            // делать НЕЛЬЗЯ — read мог бы зависнуть, держи пайп внук.
            kill_process_tree(&mut child);
            drop(stdout_handle);
            drop(stderr_handle);
            let _ = fs::remove_file(&codex_out_file);
            return Ok(None);
        }

        match child.try_wait()? {
            Some(status) => {
                let stdout = stdout_handle
                    .map(|handle| handle.join().unwrap_or_default())
                    .unwrap_or_default();
                let _ = stderr_handle.map(|handle| handle.join().unwrap_or_default());

                let (text, usage, is_error) = if provider == "claude" {
                    let parsed = parse_claude_response(&stdout);
                    (parsed.text, parsed.usage, parsed.is_error)
                } else {
                    let text = fs::read_to_string(&codex_out_file).unwrap_or_default();
                    let usage = parse_codex_usage(&stdout);
                    let _ = fs::remove_file(&codex_out_file);
                    (text, usage, false)
                };

                let mut code = status.code().unwrap_or(1);
                if is_error && code == 0 {
                    code = 1;
                }
                return Ok(Some(TandemStep { text, code, usage }));
            }
            None => {
                if last_activity
                    .lock()
                    .map(|t| t.elapsed())
                    .unwrap_or_default()
                    >= idle_timeout()
                {
                    // Зависший CLI: убиваем всю его группу (CLI + под-процессы), чтобы
                    // закрылись пайпы, и роняем ридеры — они завершатся сами по EOF.
                    kill_process_tree(&mut child);
                    drop(stdout_handle);
                    drop(stderr_handle);
                    let _ = fs::remove_file(&codex_out_file);
                    let secs = idle_timeout().as_secs();
                    let text = format!(
                        "{} {secs}{}",
                        lang.choose("Провайдер не отвечал", "Provider produced no output for"),
                        lang.choose(" c — остановлен по таймауту.", "s — stopped (timeout)."),
                    );
                    return Ok(Some(TandemStep {
                        text,
                        code: 124,
                        usage: None,
                    }));
                }
                thread::sleep(Duration::from_millis(80));
            }
        }
    }
}

fn tandem_accumulate(total: &mut RunUsage, usage: &Option<RunUsage>) {
    if let Some(u) = usage {
        total.input += u.input;
        total.output += u.output;
        total.cache_read += u.cache_read;
        total.cache_creation += u.cache_creation;
        total.cost_usd += u.cost_usd;
    }
}

fn emit_tandem_step(tx: &Sender<WorkerEvent>, marker: &str, who: &str, phase: &str, text: &str) {
    // Пустая строка-разделитель ПЕРЕД шагом, а не после: иначе последний шаг
    // оставляет хвостовую пустую строку, и над inactive-лоадером получается двойной
    // отступ (хвост шага + gap_top).
    let _ = tx.send(WorkerEvent::ChatLine(String::new()));
    let _ = tx.send(WorkerEvent::ChatLine(format!("{marker} {who} · {phase}")));
    for line in text.trim().lines() {
        let _ = tx.send(WorkerEvent::ChatLine(line.to_string()));
    }
}

fn tandem_notice(tx: &Sender<WorkerEvent>, text: String) {
    let _ = tx.send(WorkerEvent::Line(text));
}

fn opt_usage(total: RunUsage) -> Option<RunUsage> {
    if total == RunUsage::default() {
        None
    } else {
        Some(total)
    }
}

/// Оркестратор тандема: дебаты до консенсуса → исполнение → ревью → правка →
/// подтверждение. Серия вызовов `run_provider_once`; стрим шагов в чат.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_tandem(
    executor: &'static str,
    critic: &'static str,
    executor_effort: &str,
    critic_effort: &str,
    task: &str,
    rounds: usize,
    work_dir: &Path,
    cancel_rx: Receiver<()>,
    tx: Sender<WorkerEvent>,
    lang: Language,
) -> io::Result<TandemResult> {
    let mut transcript = TandemTranscript::new();
    let mut total = RunUsage::default();
    let executor_name = provider_display(executor, lang);
    let critic_name = provider_display(critic, lang);
    let exec_role = lang.choose("Исполнитель", "Executor");
    let crit_role = lang.choose("Критик", "Critic");

    // P5: предупреждение о возможных изменённых файлах при прерывании после исполнения.
    let dirty_notice = |tx: &Sender<WorkerEvent>| {
        tandem_notice(
            tx,
            lang.choose(
                "⚠ Файлы были изменены до прерывания — проверь рабочую директорию.",
                "⚠ Files were modified before interruption — check the working directory.",
            )
            .to_string(),
        );
    };

    // ФАЗА ДЕБАТОВ
    let mut consensus = false;
    for round in 1..=rounds.max(1) {
        let propose = tandem_propose_prompt(task, &transcript.render(), lang);
        let step = match run_provider_once(
            executor,
            executor_effort,
            &propose,
            work_dir,
            RunAccess::PlanReadonly,
            lang,
            &tx,
            &cancel_rx,
        )? {
            Some(s) => s,
            None => return Ok(TandemResult::Cancelled),
        };
        tandem_accumulate(&mut total, &step.usage);
        if step.code != 0 {
            tandem_notice(
                &tx,
                format!(
                    "{} {}",
                    executor_name,
                    lang.choose("вернул ошибку", "returned an error")
                ),
            );
            return Ok(TandemResult::Completed(step.code, opt_usage(total)));
        }
        emit_tandem_step(
            &tx,
            "🅐",
            executor_name,
            &format!("{} {round} · {}", lang.choose("раунд", "round"), exec_role),
            &step.text,
        );
        transcript.push(
            exec_role,
            &format!(
                "{} {round}",
                lang.choose("предложение, раунд", "proposal, round")
            ),
            &step.text,
        );

        let challenge = tandem_challenge_prompt(task, &transcript.render(), lang);
        let step = match run_provider_once(
            critic,
            critic_effort,
            &challenge,
            work_dir,
            RunAccess::PlanReadonly,
            lang,
            &tx,
            &cancel_rx,
        )? {
            Some(s) => s,
            None => return Ok(TandemResult::Cancelled),
        };
        tandem_accumulate(&mut total, &step.usage);
        if step.code != 0 {
            tandem_notice(
                &tx,
                format!(
                    "{} {}",
                    critic_name,
                    lang.choose("вернул ошибку", "returned an error")
                ),
            );
            return Ok(TandemResult::Completed(step.code, opt_usage(total)));
        }
        emit_tandem_step(
            &tx,
            "🅒",
            critic_name,
            &format!("{} {round} · {}", lang.choose("раунд", "round"), crit_role),
            &step.text,
        );
        transcript.push(
            crit_role,
            &format!(
                "{} {round}",
                lang.choose("критика, раунд", "critique, round")
            ),
            &step.text,
        );

        if parse_tandem_signal(&step.text) {
            consensus = true;
            break;
        }
    }
    if !consensus {
        tandem_notice(
            &tx,
            lang.choose(
                "⚠ Консенсус не достигнут за раунды — исполняю последнюю версию.",
                "⚠ No consensus within the rounds — executing the latest proposal.",
            )
            .to_string(),
        );
    }

    // ФАЗА ИСПОЛНЕНИЯ
    if cancel_rx.try_recv().is_ok() {
        return Ok(TandemResult::Cancelled);
    }
    let execute = tandem_execute_prompt(task, &transcript.render(), lang);
    let step = match run_provider_once(
        executor,
        executor_effort,
        &execute,
        work_dir,
        RunAccess::PlanExecute,
        lang,
        &tx,
        &cancel_rx,
    )? {
        Some(s) => s,
        None => {
            dirty_notice(&tx);
            return Ok(TandemResult::Cancelled);
        }
    };
    tandem_accumulate(&mut total, &step.usage);
    if step.code != 0 {
        dirty_notice(&tx);
        tandem_notice(
            &tx,
            format!(
                "{} {}",
                executor_name,
                lang.choose("вернул ошибку", "returned an error")
            ),
        );
        return Ok(TandemResult::Completed(step.code, opt_usage(total)));
    }
    emit_tandem_step(
        &tx,
        "🅐",
        executor_name,
        &format!("{} · {}", lang.choose("исполнение", "execution"), exec_role),
        &step.text,
    );
    transcript.push(
        exec_role,
        lang.choose("исполнение", "execution"),
        &step.text,
    );

    // ФАЗА РЕВЬЮ
    let review = tandem_review_prompt(task, &transcript.render(), lang);
    let step = match run_provider_once(
        critic,
        critic_effort,
        &review,
        work_dir,
        RunAccess::PlanReadonly,
        lang,
        &tx,
        &cancel_rx,
    )? {
        Some(s) => s,
        None => {
            dirty_notice(&tx);
            return Ok(TandemResult::Cancelled);
        }
    };
    tandem_accumulate(&mut total, &step.usage);
    emit_tandem_step(
        &tx,
        "🅒",
        critic_name,
        &format!("{} · {}", lang.choose("ревью", "review"), crit_role),
        &step.text,
    );
    transcript.push(crit_role, lang.choose("ревью", "review"), &step.text);
    let review_ok = step.code == 0 && parse_tandem_signal(&step.text);

    // ФИНАЛЬНАЯ ПРАВКА + ПОДТВЕРЖДЕНИЕ (P4)
    if !review_ok {
        let review_text = step.text.clone();
        if cancel_rx.try_recv().is_ok() {
            dirty_notice(&tx);
            return Ok(TandemResult::Cancelled);
        }
        let fix = tandem_fix_prompt(task, &transcript.render(), &review_text, lang);
        let step = match run_provider_once(
            executor,
            executor_effort,
            &fix,
            work_dir,
            RunAccess::PlanExecute,
            lang,
            &tx,
            &cancel_rx,
        )? {
            Some(s) => s,
            None => {
                dirty_notice(&tx);
                return Ok(TandemResult::Cancelled);
            }
        };
        tandem_accumulate(&mut total, &step.usage);
        emit_tandem_step(
            &tx,
            "🅐",
            executor_name,
            &format!(
                "{} · {}",
                lang.choose("финальная правка", "final fix"),
                exec_role
            ),
            &step.text,
        );
        transcript.push(
            exec_role,
            lang.choose("финальная правка", "final fix"),
            &step.text,
        );

        let confirm = tandem_confirm_prompt(task, &transcript.render(), lang);
        let step = match run_provider_once(
            critic,
            critic_effort,
            &confirm,
            work_dir,
            RunAccess::PlanReadonly,
            lang,
            &tx,
            &cancel_rx,
        )? {
            Some(s) => s,
            None => {
                dirty_notice(&tx);
                return Ok(TandemResult::Cancelled);
            }
        };
        tandem_accumulate(&mut total, &step.usage);
        emit_tandem_step(
            &tx,
            "🅒",
            critic_name,
            &format!(
                "{} · {}",
                lang.choose("подтверждение", "confirmation"),
                crit_role
            ),
            &step.text,
        );
        if !parse_tandem_signal(&step.text) {
            tandem_notice(
                &tx,
                lang.choose(
                    "⚠ Остались замечания критика.",
                    "⚠ The critic still has unresolved issues.",
                )
                .to_string(),
            );
        }
    }

    Ok(TandemResult::Completed(0, opt_usage(total)))
}

pub(crate) struct ChatResponse {
    pub(crate) text: String,
    pub(crate) usage: Option<RunUsage>,
    pub(crate) is_error: bool,
}

/// Разобрать ответ `claude -p --output-format json`. При невалидном JSON —
/// fallback: весь stdout как текст, usage отсутствует.
pub(crate) fn parse_claude_response(stdout: &str) -> ChatResponse {
    let trimmed = stdout.trim();
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(value) => {
            let text = value
                .get("result")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let is_error = value
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let usage = value.get("usage").map(|u| RunUsage {
                input: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                output: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                cache_read: u
                    .get("cache_read_input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                cache_creation: u
                    .get("cache_creation_input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                cost_usd: value
                    .get("total_cost_usd")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0),
            });
            ChatResponse {
                text,
                usage,
                is_error,
            }
        }
        Err(_) => ChatResponse {
            text: trimmed.to_string(),
            usage: None,
            is_error: false,
        },
    }
}

/// Рекурсивно ищем объект с токенами (имена полей различаются между версиями codex).
fn find_token_usage(value: &serde_json::Value) -> Option<RunUsage> {
    let input = value
        .get("input_tokens")
        .or_else(|| value.get("prompt_tokens"))
        .and_then(|v| v.as_u64());
    let output = value
        .get("output_tokens")
        .or_else(|| value.get("completion_tokens"))
        .and_then(|v| v.as_u64());
    if let (Some(input), Some(output)) = (input, output) {
        let cache_read = value
            .get("cached_input_tokens")
            .or_else(|| value.get("cache_read_input_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        return Some(RunUsage {
            input,
            output,
            cache_read,
            cache_creation: 0,
            cost_usd: 0.0,
        });
    }
    match value {
        serde_json::Value::Object(map) => map.values().find_map(find_token_usage),
        serde_json::Value::Array(items) => items.iter().find_map(find_token_usage),
        _ => None,
    }
}

/// Разобрать JSONL событий `codex exec --json`, вернуть последний найденный usage.
/// codex не сообщает стоимость, поэтому cost_usd = 0.0.
pub(crate) fn parse_codex_usage(jsonl: &str) -> Option<RunUsage> {
    let mut last = None;
    for line in jsonl.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(usage) = find_token_usage(&value) {
                last = Some(usage);
            }
        }
    }
    last
}

/// Активность из события codex `item.started`: команду показываем детально, прочие
/// типы (reasoning, agent_message, file_change, …) — обобщённо, чтобы codex-прогон
/// не выглядел «просто спиннером» (раньше активность была только для команд).
fn codex_activity(value: &serde_json::Value, lang: Language) -> Option<String> {
    if value.get("type")?.as_str()? != "item.started" {
        return None;
    }
    let item = value.get("item")?;
    match item.get("type").and_then(|v| v.as_str()).unwrap_or("") {
        "command_execution" => item
            .get("command")
            .and_then(|v| v.as_str())
            .map(|command| summarize_codex_command(command, lang)),
        "reasoning" => Some(lang.choose("Рассуждаю…", "Reasoning…").to_string()),
        "agent_message" | "assistant_message" => {
            Some(lang.choose("Пишу ответ…", "Writing answer…").to_string())
        }
        "file_change" | "patch" => Some(lang.choose("Правлю файлы", "Editing files").to_string()),
        "mcp_tool_call" | "tool_call" => Some(
            lang.choose("Вызываю инструмент", "Calling a tool")
                .to_string(),
        ),
        "" => None,
        other => Some(format!("⚙ {other}")),
    }
}

fn codex_path_token(command: &str) -> Option<String> {
    command
        .split_whitespace()
        .rev()
        .map(|token| token.trim_matches(|c| c == '"' || c == '\''))
        .find(|token| token.contains('/') || token.contains('.'))
        .map(String::from)
}

/// Превратить shell-команду codex в короткую человекочитаемую активность для лоадера.
pub(crate) fn summarize_codex_command(command: &str, lang: Language) -> String {
    let inner = command
        .split_once("-lc")
        .map(|(_, rest)| rest.trim().trim_matches('"').trim().to_string())
        .unwrap_or_else(|| command.to_string());
    let first = inner.split_whitespace().next().unwrap_or("").to_lowercase();

    if matches!(
        first.as_str(),
        "sed" | "cat" | "head" | "tail" | "less" | "bat" | "more"
    ) {
        return match codex_path_token(&inner) {
            Some(file) => format!("{} {}", lang.choose("Читаю", "Reading"), file),
            None => lang.choose("Читаю файл", "Reading file").to_string(),
        };
    }
    if matches!(first.as_str(), "grep" | "rg" | "ag" | "ack") {
        return lang.choose("Ищу по коду", "Searching code").to_string();
    }
    if matches!(first.as_str(), "ls" | "find" | "fd" | "tree") {
        return lang
            .choose("Просматриваю файлы", "Listing files")
            .to_string();
    }
    format!("⚙ {}", truncate_chars(&inner, 60))
}

/// Потоково читает JSONL codex: эмитит активность (command_execution) в лоадер
/// и возвращает весь stdout (для разбора usage в конце).
pub(crate) fn spawn_codex_activity_reader(
    reader: impl Read + Send + 'static,
    tx: Sender<WorkerEvent>,
    lang: Language,
    last_activity: Arc<Mutex<Instant>>,
) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let reader = BufReader::new(reader);
        let mut full = String::new();
        for line in reader.lines().map_while(Result::ok) {
            touch_activity(&last_activity);
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                if let Some(activity) = codex_activity(&value, lang) {
                    let _ = tx.send(WorkerEvent::Activity(activity));
                }
            }
            full.push_str(&line);
            full.push('\n');
        }
        full
    })
}

fn short_path(path: &str) -> String {
    let tail: Vec<&str> = path.rsplit('/').take(2).collect();
    tail.into_iter().rev().collect::<Vec<_>>().join("/")
}

/// Превратить claude tool_use в короткую человекочитаемую активность для лоадера.
fn summarize_claude_tool(item: &serde_json::Value, lang: Language) -> Option<String> {
    let name = item.get("name")?.as_str()?;
    let input = item.get("input");
    let path = input
        .and_then(|i| i.get("file_path"))
        .and_then(|v| v.as_str())
        .map(short_path);
    let command = input
        .and_then(|i| i.get("command"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let pattern = input
        .and_then(|i| i.get("pattern"))
        .and_then(|v| v.as_str());
    let summary = match name {
        "Read" | "NotebookRead" => {
            format!(
                "{} {}",
                lang.choose("Читаю", "Reading"),
                path.unwrap_or_default()
            )
        }
        "Edit" | "MultiEdit" | "NotebookEdit" => {
            format!(
                "{} {}",
                lang.choose("Правлю", "Editing"),
                path.unwrap_or_default()
            )
        }
        "Write" => format!(
            "{} {}",
            lang.choose("Создаю", "Writing"),
            path.unwrap_or_default()
        ),
        "Bash" => format!(
            "{} {}",
            lang.choose("Выполняю", "Running"),
            truncate_chars(command, 50)
        ),
        "Grep" => match pattern {
            Some(p) => format!(
                "{} {}",
                lang.choose("Ищу", "Searching"),
                truncate_chars(p, 40)
            ),
            None => lang.choose("Ищу по коду", "Searching code").to_string(),
        },
        "Glob" => match pattern {
            Some(p) => format!(
                "{} {}",
                lang.choose("Просматриваю", "Listing"),
                truncate_chars(p, 40)
            ),
            None => lang
                .choose("Просматриваю файлы", "Listing files")
                .to_string(),
        },
        other => format!("⚙ {other}"),
    };
    Some(summary)
}

/// Достаёт инкремент текста ответа из события claude (`--include-partial-messages`):
/// либо сам объект — `content_block_delta`, либо завёрнут в `stream_event.event`.
/// Берём только `text_delta` (не thinking/signature).
fn claude_text_delta(value: &serde_json::Value) -> Option<String> {
    let block = match value.get("type").and_then(|v| v.as_str()) {
        Some("stream_event") => value.get("event")?,
        _ => value,
    };
    if block.get("type").and_then(|v| v.as_str()) != Some("content_block_delta") {
        return None;
    }
    let delta = block.get("delta")?;
    if delta.get("type").and_then(|v| v.as_str()) != Some("text_delta") {
        return None;
    }
    delta
        .get("text")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// Дельта рассуждения (`thinking_delta`) из стрима claude — то же ограждение, что и
/// для текста ответа, но поле `thinking` вместо `text`. Пусто, если thinking выключен.
fn claude_thinking_delta(value: &serde_json::Value) -> Option<String> {
    let block = match value.get("type").and_then(|v| v.as_str()) {
        Some("stream_event") => value.get("event")?,
        _ => value,
    };
    if block.get("type").and_then(|v| v.as_str()) != Some("content_block_delta") {
        return None;
    }
    let delta = block.get("delta")?;
    if delta.get("type").and_then(|v| v.as_str()) != Some("thinking_delta") {
        return None;
    }
    delta
        .get("thinking")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// Потоково читает claude stream-json: токены ответа эмитит как StreamDelta (живой
/// вывод), активность tool_use — в лоадер, и возвращает финальное result-событие.
pub(crate) fn spawn_claude_activity_reader(
    reader: impl Read + Send + 'static,
    tx: Sender<WorkerEvent>,
    lang: Language,
    last_activity: Arc<Mutex<Instant>>,
) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let reader = BufReader::new(reader);
        let mut result_line = String::new();
        for line in reader.lines().map_while(Result::ok) {
            touch_activity(&last_activity);
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if let Some(delta) = claude_text_delta(&value) {
                let _ = tx.send(WorkerEvent::StreamDelta(delta));
                continue;
            }
            // Рассуждение (extended thinking при высоком effort) — отдельным потоком
            // в лоадер, чтобы было видно, как модель думает до ответа.
            if let Some(delta) = claude_thinking_delta(&value) {
                let _ = tx.send(WorkerEvent::ReasoningDelta(delta));
                continue;
            }
            match value.get("type").and_then(|v| v.as_str()) {
                Some("assistant") => {
                    if let Some(content) = value
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(|c| c.as_array())
                    {
                        for item in content {
                            if item.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                                if let Some(activity) = summarize_claude_tool(item, lang) {
                                    let _ = tx.send(WorkerEvent::Activity(activity));
                                }
                            }
                        }
                    }
                }
                Some("result") => result_line = line.clone(),
                _ => {}
            }
        }
        result_line
    })
}

pub(crate) fn spawn_capture_reader<R>(reader: R) -> thread::JoinHandle<String>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut text = String::new();
        let _ = reader.read_to_string(&mut text);
        text
    })
}

pub(crate) fn emit_chat_lines(tx: &Sender<WorkerEvent>, text: &str) {
    let mut first_content = true;
    for line in text.lines() {
        let rendered = if first_content && !line.trim().is_empty() {
            first_content = false;
            format!("⏺ {}", line.trim_start())
        } else {
            line.to_string()
        };
        let _ = tx.send(WorkerEvent::ChatLine(rendered));
    }
}

/// Строки для показа при ошибке провайдера в чате: заголовок с КОДОМ выхода, затем
/// детали из stderr — а если stderr пуст (claude шлёт ошибки в stdout stream-json и
/// при обрыве до `result` они не доезжают), честная подсказка о транзиентной природе
/// сбоя вместо немого «no stderr output».
pub(crate) fn chat_error_lines(
    provider: &'static str,
    code: i32,
    stderr: &str,
    lang: Language,
) -> Vec<String> {
    let mut out = vec![format!(
        "{} {} ({} {code}):",
        provider_display(provider, lang),
        lang.choose("вернул ошибку", "returned an error"),
        lang.choose("код", "exit code"),
    )];

    let stderr = stderr.trim();
    if stderr.is_empty() {
        out.push(
            lang.choose(
                "⎿ без вывода — вероятно транзиентный сбой (сеть / лимит / таймаут). Повтори запрос.",
                "⎿ no output — likely a transient failure (network / rate limit / timeout). Try again.",
            )
            .to_string(),
        );
    } else {
        for line in stderr
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(40)
        {
            out.push(format!("⎿ {line}"));
        }
    }
    out
}

pub(crate) fn engine_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("CLAVE_ENGINE") {
        if let Some(path) = existing_path(PathBuf::from(path)) {
            return Some(path);
        }
    }

    if let Ok(current_dir) = env::current_dir() {
        if let Some(path) = existing_path(current_dir.join(ENGINE_NAME)) {
            return Some(path);
        }
    }

    if let Ok(exe) = env::current_exe() {
        for dir in exe.ancestors().skip(1).take(4) {
            if let Some(path) = existing_path(dir.join(ENGINE_NAME)) {
                return Some(path);
            }
        }
    }

    // Последний фолбэк: движок вшит в бинарник. Установленный через `cargo install`
    // `clave` живёт один (без скриптов рядом) — распаковываем встроенную копию в
    // кэш состояния и работаем с ней. В dev-чекауте сюда не доходим: скрипты
    // находятся в cwd/рядом с exe выше, и правки видны сразу.
    embedded_engine_path()
}

/// Движок, вшитый на этапе компиляции (путь — от src/ к корню репозитория).
const EMBEDDED_SPEC_CLAVE: &str = include_str!("../spec-clave");

/// Путь к распакованной встроенной копии движка (`spec-clave`).
fn embedded_engine_path() -> Option<PathBuf> {
    extract_engine_to(&clave_state_dir().join("engine"))
}

/// Распаковывает вшитый движок в `dir` (идемпотентно, по «штампу» содержимого) и
/// возвращает путь к `spec-clave`.
fn extract_engine_to(dir: &Path) -> Option<PathBuf> {
    let engine = dir.join(ENGINE_NAME);
    let stamp_path = dir.join(".stamp");
    let want = engine_stamp();

    // Перезаписываем только если содержимое сменилось (обновление бинарника) или
    // файла нет — иначе не трогаем диск на каждом запуске плана.
    let fresh = engine.exists() && fs::read_to_string(&stamp_path).is_ok_and(|s| s.trim() == want);
    if !fresh {
        fs::create_dir_all(dir).ok()?;
        write_engine_file(&engine, EMBEDDED_SPEC_CLAVE)?;
        let _ = fs::write(&stamp_path, &want);
    }
    existing_path(engine)
}

/// Записывает файл движка и на unix ставит исполняемый бит (shebang сам по себе не
/// делает файл исполняемым). На Windows бит не нужен — `/plan` там идёт через bash
/// (WSL/Git Bash), а сам файл всё равно читается интерпретатором.
fn write_engine_file(path: &Path, content: &str) -> Option<()> {
    fs::write(path, content).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o755));
    }
    Some(())
}

/// Короткий «штамп» содержимого движка (FNV-1a, без внешних зависимостей):
/// меняется при правке движка → распаковка обновит файл.
fn engine_stamp() -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in EMBEDDED_SPEC_CLAVE.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

pub(crate) fn existing_path(path: PathBuf) -> Option<PathBuf> {
    if !path.exists() {
        return None;
    }
    Some(path.canonicalize().unwrap_or(path))
}

pub(crate) fn launch_work_dir() -> PathBuf {
    env::var("CLAVE_LAUNCH_CWD")
        .ok()
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .and_then(existing_path)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub(crate) fn resolve_work_dir(configured: &str, base_dir: &Path) -> PathBuf {
    let configured = configured.trim();
    if configured.is_empty() || configured == "." {
        return base_dir.to_path_buf();
    }

    let path = PathBuf::from(configured);
    let resolved = if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    };

    if resolved.is_dir() {
        resolved.canonicalize().unwrap_or(resolved)
    } else {
        base_dir.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_engine_extracts_runnable_script() {
        // Имитируем установленный бинарник без скриптов рядом: распаковка вшитой копии.
        let dir = env::temp_dir().join("clave-engine-embed-test");
        let _ = fs::remove_dir_all(&dir);

        let path = extract_engine_to(&dir).expect("движок распаковывается");
        assert!(
            path.ends_with(ENGINE_NAME),
            "вернули путь к движку: {path:?}"
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            EMBEDDED_SPEC_CLAVE,
            "содержимое spec-clave совпадает с вшитым"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert!(mode & 0o111 != 0, "spec-clave исполняемый: {mode:o}");
        }

        // Идемпотентность: повторная распаковка не падает и даёт тот же путь.
        assert_eq!(extract_engine_to(&dir).expect("повторно"), path);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn claude_text_delta_extracts_only_streamed_answer_text() {
        // Текст-дельта, завёрнутая в stream_event → берём.
        let wrapped = serde_json::json!({"type":"stream_event","event":{
            "type":"content_block_delta","index":0,
            "delta":{"type":"text_delta","text":"привет"}}});
        assert_eq!(claude_text_delta(&wrapped).as_deref(), Some("привет"));
        // Размышления (thinking) и финальный result — НЕ стримим как ответ.
        let thinking = serde_json::json!({"type":"stream_event","event":{
            "type":"content_block_delta","delta":{"type":"thinking_delta","text":"гм"}}});
        assert_eq!(claude_text_delta(&thinking), None);
        assert_eq!(
            claude_text_delta(&serde_json::json!({"type":"result","result":"x"})),
            None
        );
    }

    #[test]
    fn claude_thinking_delta_extracts_reasoning_only() {
        // thinking_delta (поле `thinking`) → берём как рассуждение.
        let think = serde_json::json!({"type":"stream_event","event":{
            "type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"прикину"}}});
        assert_eq!(claude_thinking_delta(&think).as_deref(), Some("прикину"));
        // Текст ответа рассуждением НЕ считаем (он идёт своим потоком).
        let text = serde_json::json!({"type":"stream_event","event":{
            "type":"content_block_delta","delta":{"type":"text_delta","text":"ответ"}}});
        assert_eq!(claude_thinking_delta(&text), None);
    }

    #[test]
    fn codex_activity_covers_item_types() {
        let item = |t: &str| serde_json::json!({"type":"item.started","item":{"type":t}});
        // Не item.started → нет активности.
        assert!(
            codex_activity(&serde_json::json!({"type":"turn.started"}), Language::Ru).is_none()
        );
        assert_eq!(
            codex_activity(&item("reasoning"), Language::Ru).as_deref(),
            Some("Рассуждаю…")
        );
        assert_eq!(
            codex_activity(&item("agent_message"), Language::Ru).as_deref(),
            Some("Пишу ответ…")
        );
        // Команда — детальная сводка (не дефолтная заглушка).
        let cmd = serde_json::json!({"type":"item.started",
            "item":{"type":"command_execution","command":"ls -la"}});
        assert!(codex_activity(&cmd, Language::Ru).is_some());
        // Неизвестный тип — обобщённо, но не молчим (codex не должен быть «спиннером»).
        assert_eq!(
            codex_activity(&item("totally_new"), Language::Ru).as_deref(),
            Some("⚙ totally_new")
        );
    }

    #[test]
    fn resolves_dot_to_launch_directory() {
        let base = env::current_dir().expect("test cwd exists");
        assert_eq!(resolve_work_dir(".", &base), base);
    }

    #[test]
    fn resolves_relative_directory_from_launch_directory() {
        let base = env::current_dir().expect("test cwd exists");
        let expected = base.join("src").canonicalize().expect("src dir exists");
        assert_eq!(resolve_work_dir("src", &base), expected);
    }

    #[test]
    fn parses_claude_json_with_usage() {
        let raw = r#"{"type":"result","is_error":false,"result":"Привет!","total_cost_usd":0.0123,"usage":{"input_tokens":120,"output_tokens":40,"cache_read_input_tokens":5,"cache_creation_input_tokens":9}}"#;
        let parsed = parse_claude_response(raw);
        assert_eq!(parsed.text, "Привет!");
        assert!(!parsed.is_error);
        let usage = parsed.usage.expect("usage present");
        assert_eq!(usage.input, 120);
        assert_eq!(usage.output, 40);
        assert_eq!(usage.cache_read, 5);
        assert_eq!(usage.cache_creation, 9);
        assert!((usage.cost_usd - 0.0123).abs() < 1e-9);
    }

    #[test]
    fn claude_parser_falls_back_on_non_json() {
        let parsed = parse_claude_response("просто текст без json");
        assert_eq!(parsed.text, "просто текст без json");
        assert!(parsed.usage.is_none());
    }

    #[test]
    fn parses_codex_usage_from_jsonl() {
        let jsonl = "{\"type\":\"item\",\"text\":\"hi\"}\n{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":200,\"output_tokens\":60,\"cached_input_tokens\":10}}\n";
        let usage = parse_codex_usage(jsonl).expect("usage found");
        assert_eq!(usage.input, 200);
        assert_eq!(usage.output, 60);
        assert_eq!(usage.cache_read, 10);
        assert_eq!(usage.cost_usd, 0.0);
    }

    #[test]
    fn codex_usage_none_when_absent() {
        let jsonl = "{\"type\":\"item\",\"text\":\"hi\"}\n";
        assert!(parse_codex_usage(jsonl).is_none());
    }

    #[test]
    fn summarizes_codex_read_command() {
        let cmd = "/bin/zsh -lc \"sed -n '1,240p' src/model/overlay.rs\"";
        assert_eq!(
            summarize_codex_command(cmd, Language::En),
            "Reading src/model/overlay.rs"
        );
        let grep = "/bin/zsh -lc \"grep -rn Overlay src\"";
        assert_eq!(
            summarize_codex_command(grep, Language::En),
            "Searching code"
        );
    }

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
    fn plan_prompt_forbids_file_changes() {
        let p = plan_prompt("add a feature", "", Language::En);
        assert!(p.contains("Do NOT modify"));
        assert!(p.contains("add a feature"));
    }

    #[test]
    fn execute_prompt_embeds_full_plan() {
        let p = execute_prompt(
            "add a feature",
            "1. first step\n2. second step",
            "",
            Language::En,
        );
        assert!(p.contains("Approved plan"));
        assert!(p.contains("first step"));
        assert!(p.contains("second step"));
    }

    #[test]
    fn refine_prompt_carries_feedback_and_prev_plan() {
        let p = refine_prompt(
            "add a feature",
            "1. old step",
            "make it simpler",
            "",
            Language::En,
        );
        assert!(p.contains("old step"));
        assert!(p.contains("make it simpler"));
        assert!(p.contains("Do NOT modify"));
    }

    #[test]
    fn tandem_signal_parses_last_marker() {
        assert!(parse_tandem_signal("bla bla\nTANDEM: CONSENSUS"));
        assert!(!parse_tandem_signal("TANDEM: CONTINUE\nmore text"));
        assert!(!parse_tandem_signal("no signal here"));
        // последний маркер решает
        assert!(!parse_tandem_signal(
            "TANDEM: CONSENSUS\n...\nTANDEM: CONTINUE"
        ));
        // строгий разбор: упоминание TANDEM: в прозе — НЕ сигнал
        assert!(!parse_tandem_signal(
            "I will output TANDEM: CONSENSUS when we are done."
        ));
        // markdown-обрамление снимается, чистый сигнал проходит
        assert!(parse_tandem_signal("**TANDEM: CONSENSUS**"));
        assert!(parse_tandem_signal("> TANDEM: CONSENSUS"));
        // хвостовой текст после сигнала не засчитывается как консенсус
        assert!(!parse_tandem_signal("TANDEM: CONSENSUS reached, ship it"));
    }

    #[test]
    fn tandem_prompts_carry_role_and_signal_rules() {
        let ch = tandem_challenge_prompt("do x", "", Language::En);
        assert!(ch.contains("CRITIC"));
        assert!(ch.contains("TANDEM: CONSENSUS"));
        assert!(ch.contains("Do NOT agree out of politeness"));

        let ex = tandem_execute_prompt("do x", "approach", Language::En);
        assert!(ex.contains("EXECUTOR"));
        assert!(ex.contains("edit files"));

        let fix = tandem_fix_prompt("do x", "", "fix the bug", Language::En);
        assert!(fix.contains("fix the bug"));
    }

    #[test]
    fn tandem_transcript_renders_and_truncates() {
        let mut t = TandemTranscript::new();
        t.push("Executor", "proposal 1", "short");
        assert!(t.render().contains("short"));
        for i in 0..60 {
            t.push("Critic", "round", &format!("entry {i} {}", "y".repeat(400)));
        }
        assert!(t.render().contains("усечены"));
    }

    #[test]
    fn summarizes_claude_tool_use() {
        let read = serde_json::json!({
            "type": "tool_use",
            "name": "Read",
            "input": {"file_path": "/Users/x/proj/src/model/overlay.rs"}
        });
        assert_eq!(
            summarize_claude_tool(&read, Language::En),
            Some("Reading model/overlay.rs".to_string())
        );

        let bash = serde_json::json!({
            "type": "tool_use",
            "name": "Bash",
            "input": {"command": "cargo build"}
        });
        assert_eq!(
            summarize_claude_tool(&bash, Language::En),
            Some("Running cargo build".to_string())
        );

        let write = serde_json::json!({
            "type": "tool_use",
            "name": "Write",
            "input": {"file_path": "/a/b/new.rs"}
        });
        assert_eq!(
            summarize_claude_tool(&write, Language::En),
            Some("Writing b/new.rs".to_string())
        );

        let grep = serde_json::json!({
            "type": "tool_use",
            "name": "Grep",
            "input": {"pattern": "TODO"}
        });
        assert_eq!(
            summarize_claude_tool(&grep, Language::En),
            Some("Searching TODO".to_string())
        );
    }

    #[test]
    fn tandem_prompts_carry_engineering_principles() {
        let marker = "Stay lean on dependencies";
        let exec = [
            tandem_propose_prompt("t", "", Language::En),
            tandem_execute_prompt("t", "", Language::En),
            tandem_fix_prompt("t", "", "r", Language::En),
        ];
        for (i, p) in exec.iter().enumerate() {
            assert!(p.contains(marker), "executor prompt #{i} has principles");
            assert!(
                p.contains("Reach for the minimal change first"),
                "executor prompt #{i} has executor discipline"
            );
        }
        let crit = [
            tandem_challenge_prompt("t", "", Language::En),
            tandem_review_prompt("t", "", Language::En),
            tandem_confirm_prompt("t", "", Language::En),
        ];
        for (i, p) in crit.iter().enumerate() {
            assert!(p.contains(marker), "critic prompt #{i} has principles");
            assert!(
                p.contains("do NOT trust the other agent's answer"),
                "critic prompt #{i} has critic discipline"
            );
        }
    }

    #[test]
    fn chat_error_lines_surface_code_and_cause() {
        // Пустой stderr (типичный claude-сбой): заголовок с кодом + подсказка о
        // транзиентной причине, БЕЗ немого «no stderr output».
        let lines = chat_error_lines("claude", 1, "  ", Language::Ru);
        assert!(
            lines[0].contains("код 1"),
            "код выхода в заголовке: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("транзиентный")),
            "подсказка о причине: {lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("no stderr output")),
            "немого сообщения больше нет"
        );

        // Непустой stderr (codex): код + строки stderr, без подсказки-заглушки.
        let lines = chat_error_lines("codex", 2, "boom: connection reset\n", Language::En);
        assert!(lines[0].contains("exit code 2"), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("boom: connection reset")));
        assert!(!lines.iter().any(|l| l.contains("transient")));
    }

    #[test]
    fn transient_failure_only_for_silent_nonzero_exit() {
        // Ретраим: мгновенный exit≠0 без ответа и без stderr.
        assert!(is_transient_chat_failure(&ChatRunResult::Completed(
            1,
            String::new(),
            "  ".to_string(),
            None
        )));
        // Не ретраим: успех; есть ответ; есть stderr (причина видна); таймаут (124).
        assert!(!is_transient_chat_failure(&ChatRunResult::Completed(
            0,
            String::new(),
            String::new(),
            None
        )));
        assert!(!is_transient_chat_failure(&ChatRunResult::Completed(
            1,
            "answer".to_string(),
            String::new(),
            None
        )));
        assert!(!is_transient_chat_failure(&ChatRunResult::Completed(
            1,
            String::new(),
            "boom".to_string(),
            None
        )));
        assert!(!is_transient_chat_failure(&ChatRunResult::Completed(
            124,
            String::new(),
            String::new(),
            None
        )));
        assert!(!is_transient_chat_failure(&ChatRunResult::Cancelled));
    }

    // Критично: при отмене/таймауте мы убиваем ВСЮ группу процессов, а не только сам
    // CLI. Модель «процесс → под-процесс в той же группе»: внук должен умереть вместе с
    // прямым потомком — иначе он держал бы stdout-пайп и тред-ридер завис бы навсегда.
    #[cfg(unix)]
    #[test]
    fn kill_process_tree_reaps_grandchild() {
        let marker = env::temp_dir().join(format!("clave-test-pgrp-{}.pid", std::process::id()));
        let _ = fs::remove_file(&marker);

        // Дочерний sh порождает фоновый sleep (внук) в той же группе и печатает его PID.
        let script = format!("sleep 30 & echo $! > {}; wait", marker.to_string_lossy());
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(script)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        configure_process_group(&mut command);
        let mut child = command.spawn().expect("sh запускается");

        let grandchild = wait_for_pid(&marker);
        assert!(
            process_alive(grandchild),
            "внук должен быть жив до kill_process_tree"
        );

        kill_process_tree(&mut child);

        // После убийства группы внук должен исчезнуть в пределах таймаута.
        let mut dead = false;
        for _ in 0..200 {
            if !process_alive(grandchild) {
                dead = true;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let _ = fs::remove_file(&marker);
        assert!(
            dead,
            "внук ({grandchild}) пережил kill_process_tree — группа не убита"
        );
    }

    #[cfg(unix)]
    fn wait_for_pid(marker: &Path) -> i32 {
        for _ in 0..300 {
            if let Ok(text) = fs::read_to_string(marker) {
                if let Ok(pid) = text.trim().parse::<i32>() {
                    if pid > 0 {
                        return pid;
                    }
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("внук не записал PID за отведённое время");
    }

    #[cfg(unix)]
    fn process_alive(pid: i32) -> bool {
        // kill(pid, 0) сигнал не шлёт — только проверяет существование процесса.
        unsafe { libc::kill(pid, 0) == 0 }
    }
}
