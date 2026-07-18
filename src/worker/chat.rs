use crate::prelude::*;
use crate::*;

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
    run_chat_retrying(
        || {
            run_chat_attempt(
                provider, effort, prompt, work_dir, &cancel_rx, &tx, lang, access,
            )
        },
        access.is_read_only(),
        &cancel_rx,
        &tx,
        lang,
    )
}

/// Решение о повторе в отрыве от запуска процессов: `attempt` — одна попытка чата
/// (в проде — `run_chat_attempt`). Шов существует ради тестов: иначе условие ретрая
/// проверяемо только реальным CLI.
fn run_chat_retrying<F>(
    mut attempt: F,
    retryable: bool,
    cancel_rx: &Receiver<()>,
    tx: &Sender<WorkerEvent>,
    lang: Language,
) -> io::Result<ChatRunResult>
where
    F: FnMut() -> io::Result<ChatRunResult>,
{
    let result = attempt()?;
    if !is_transient_chat_failure(&result) {
        return Ok(result);
    }
    // Мутирующий ран НЕ ретраим: первая попытка могла уже применить необратимые
    // побочные эффекты (правки файлов, Bash, git-коммит), а «пустой result» при
    // обрыве до финального события неотличим от «ничего не сделал» — повтор
    // применил бы их дважды и мог бы испортить рабочую директорию.
    if !retryable {
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
    attempt()
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
    let codex_out = TempOut::new("codex");
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
            // Убиваем всю группу (CLI + его под-процессы) и роняем ридеры: после смерти
            // группы пайпы закрываются, треды-ридеры завершатся сами по EOF. join здесь
            // делать НЕЛЬЗЯ — read мог бы зависнуть, держи пайп внук.
            kill_process_tree(&mut child);
            drop(stdout_handle);
            drop(stderr_handle);
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

                let (text, usage, is_error) = provider_result(provider, &stdout, codex_out.read());
                let code = final_code(status.code(), is_error);
                return Ok(ChatRunResult::Completed(code, text, stderr, usage));
            }
            None => {
                if idle_expired(idle_elapsed(&last_activity), idle_timeout()) {
                    // Зависший CLI: убиваем всю его группу (CLI + под-процессы), чтобы
                    // закрылись пайпы, и роняем ридеры — они завершатся сами по EOF.
                    kill_process_tree(&mut child);
                    drop(stdout_handle);
                    drop(stderr_handle);
                    return Ok(ChatRunResult::Completed(
                        124,
                        String::new(),
                        idle_timeout_message(lang),
                        None,
                    ));
                }
                thread::sleep(Duration::from_millis(80));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // --- жизненный цикл процесса: чистые решения ---

    fn completed(code: i32, text: &str, stderr: &str) -> ChatRunResult {
        ChatRunResult::Completed(code, text.to_string(), stderr.to_string(), None)
    }

    #[test]
    fn chat_retries_silent_failure_exactly_once() {
        let (tx, rx) = mpsc::channel();
        let (_cancel_tx, cancel_rx) = mpsc::channel();
        let mut calls = 0;
        let result = run_chat_retrying(
            || {
                calls += 1;
                Ok(if calls == 1 {
                    completed(1, "", "")
                } else {
                    completed(0, "ответ", "")
                })
            },
            true,
            &cancel_rx,
            &tx,
            Language::Ru,
        )
        .expect("повтор не падает");

        assert_eq!(calls, 2, "молчаливый сбой повторяется ровно один раз");
        match result {
            ChatRunResult::Completed(code, text, ..) => {
                assert_eq!(code, 0);
                assert_eq!(text, "ответ", "возвращается результат ПОВТОРА");
            }
            ChatRunResult::Cancelled => panic!("не отменяли"),
        }
        drop(tx);
        let notices: Vec<String> = rx
            .iter()
            .filter_map(|event| match event {
                WorkerEvent::Line(line) => Some(line),
                _ => None,
            })
            .collect();
        assert!(
            notices.iter().any(|line| line.contains("повтор")),
            "пользователь видит, что идёт повтор: {notices:?}"
        );
    }

    #[test]
    fn chat_does_not_retry_a_mutating_run() {
        // Мутирующий ран (retryable=false): даже транзиентный молчаливый сбой НЕ
        // повторяется — иначе уже применённые правки/Bash/git-коммит ушли бы дважды.
        let (tx, _rx) = mpsc::channel();
        let (_cancel_tx, cancel_rx) = mpsc::channel();
        let mut calls = 0;
        let result = run_chat_retrying(
            || {
                calls += 1;
                Ok(completed(1, "", ""))
            },
            false,
            &cancel_rx,
            &tx,
            Language::Ru,
        )
        .expect("прогон не падает");
        assert_eq!(calls, 1, "мутирующий ран не повторяется");
        assert!(matches!(result, ChatRunResult::Completed(1, ..)));
    }

    #[test]
    fn chat_does_not_retry_success_timeout_or_after_cancel() {
        let attempts = |result: ChatRunResult, cancelled: bool| -> usize {
            let (tx, _rx) = mpsc::channel();
            let (cancel_tx, cancel_rx) = mpsc::channel();
            if cancelled {
                cancel_tx.send(()).expect("отмена уходит в канал");
            }
            let mut calls = 0;
            let mut once = Some(result);
            run_chat_retrying(
                || {
                    calls += 1;
                    Ok(once.take().unwrap_or_else(|| completed(0, "повтор", "")))
                },
                true,
                &cancel_rx,
                &tx,
                Language::Ru,
            )
            .expect("прогон не падает");
            calls
        };

        assert_eq!(attempts(completed(0, "ответ", ""), false), 1, "успех");
        assert_eq!(attempts(completed(1, "", "boom"), false), 1, "есть stderr");
        assert_eq!(attempts(completed(1, "ответ", ""), false), 1, "есть ответ");
        assert_eq!(attempts(completed(124, "", ""), false), 1, "таймаут");
        assert_eq!(
            attempts(ChatRunResult::Cancelled, false),
            1,
            "отменённый прогон"
        );
        // Транзиентный сбой, но пользователь уже прервал — повтора быть не должно.
        assert_eq!(attempts(completed(1, "", ""), true), 1, "отмена до повтора");
    }

    // --- оркестрация тандема ---
}
