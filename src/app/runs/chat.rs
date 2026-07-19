use super::*;

impl App {
    pub(crate) fn start_chat(&mut self, message: String) {
        // Во время выполнения сообщение не теряется, а встаёт в очередь и
        // запускается после завершения текущего рана (см. process_pending_messages).
        if self.running {
            let preview = truncate_chars(&message, 80);
            self.pending_messages.push_back(message);
            self.push_system(format!(
                "⧖ {}: {preview}",
                self.lang.choose("в очереди", "queued")
            ));
            return;
        }

        self.last_chat_message = Some(message.clone());
        match self.chat_mode {
            ChatMode::Plan => self.start_plan(message),
            ChatMode::Tandem => self.start_tandem(message),
            _ => self.start_chat_with_prompt(message.clone(), message),
        }
    }

    /// Запускает следующее сообщение из очереди, если ничего не выполняется и не
    /// открыт гейт плана. Вызывается после завершения рана.
    pub(crate) fn process_pending_messages(&mut self) {
        if self.running || self.plan_gate_active() || self.ask_active() {
            return;
        }
        if let Some(next) = self.pending_messages.pop_front() {
            self.start_chat(next);
        }
    }

    pub(crate) fn start_chat_with_prompt(&mut self, display_message: String, message: String) {
        // Запоминаем исходный текст: при отмене (Ctrl+C) вернём его в инпут.
        self.restore_on_cancel = Some(message.clone());
        let context = recent_chat_context(&self.transcript, 40);
        let prompt = chat_prompt(&message, &context, self.lang, self.chat_mode);
        self.run_provider_chat(
            format!("◆ {display_message}"),
            prompt,
            RunAccess::Chat(self.chat_mode),
            false,
        );
    }

    /// Единая точка запуска провайдера как агента. `planning = true` → завершение
    /// уходит как `PlanReady` (фаза 1 плана), иначе `ChatDone` (обычный чат и фаза 2).
    pub(crate) fn run_provider_chat(
        &mut self,
        display: String,
        prompt: String,
        access: RunAccess,
        planning: bool,
    ) {
        if self.running {
            self.push_system(
                self.lang
                    .choose("Clave уже выполняется.", "Clave is already running."),
            );
            return;
        }

        if let Some(title) = display.strip_prefix("◆ ") {
            self.set_chat_title_from_prompt_if_needed(title);
        }

        // Проверку логина НЕ делаем здесь синхронно (она спавнит CLI-подпроцессы и
        // морозит UI на пару секунд) — она ушла в воркер ниже. Сообщение и лоадер
        // показываются мгновенно.
        let provider = self.direct_provider;
        let provider_name = provider_display(provider.as_str(), self.lang);
        let effort = self.provider_effort(provider.as_str()).to_string();
        let lang = self.lang;
        let token_estimate = estimate_tokens(&prompt);
        let work_dir = self.resolved_work_dir();
        let (cancel_tx, cancel_rx) = mpsc::channel();

        self.running = true;
        self.run_started_at = Some(Instant::now());
        self.last_run_duration = None;
        self.run_label = provider_name.to_string();
        self.run_token_estimate = Some(token_estimate);
        // Лоадер стартует чистым (только спиннер) — реальная активность модели
        // подтянется по ходу через WorkerEvent::Activity.
        self.run_activity.clear();
        self.live_answer.clear();
        self.live_reasoning.clear();
        self.cancel_tx = Some(cancel_tx);
        self.last_ctrl_c_at = None;
        self.status = format!("{}...", provider_name.to_lowercase());
        // Чат: реплику держим в живом блоке (live_turn), а не в ленте — чтобы при
        // отмене её можно было убрать без следа (в ленте она уже ушла бы в
        // нативный скроллбэк). План показываем сразу, как раньше.
        if planning {
            self.push_system(display);
        } else {
            self.live_turn = Some(display);
        }

        let tx = self.tx.clone();
        let authenticated = self.run_hooks.authenticated;
        (self.run_hooks.spawn)(
            self.tx.clone(),
            Box::new(move || {
                run_chat_worker_body(
                    authenticated(provider),
                    provider,
                    || {
                        run_chat_provider(
                            provider.as_str(),
                            &effort,
                            &prompt,
                            &work_dir,
                            cancel_rx,
                            tx.clone(),
                            lang,
                            access,
                        )
                    },
                    planning,
                    lang,
                    &tx,
                );
            }),
        );
    }
}

/// Тело чат-воркера в отрыве от спавна: гейт логина → запуск → эмит результата.
/// Вынесено, чтобы `!chat_auth_ok` проверялось без реального подпроцесса.
fn run_chat_worker_body(
    authenticated: bool,
    provider: Provider,
    run: impl FnOnce() -> io::Result<ChatRunResult>,
    planning: bool,
    lang: Language,
    tx: &Sender<WorkerEvent>,
) {
    if !chat_auth_ok(authenticated, provider, tx) {
        return;
    }
    let result = run();
    emit_chat_run_result(result, provider, planning, lang, tx);
}

fn chat_auth_ok(authenticated: bool, provider: Provider, tx: &Sender<WorkerEvent>) -> bool {
    if !authenticated {
        let _ = tx.send(WorkerEvent::AuthMissing(provider));
        return false;
    }
    true
}

fn emit_chat_run_result(
    result: io::Result<ChatRunResult>,
    provider: Provider,
    planning: bool,
    lang: Language,
    tx: &Sender<WorkerEvent>,
) {
    match result {
        Ok(ChatRunResult::Completed(code, stdout, stderr, usage)) => {
            let stdout = stdout.trim();
            let stderr = stderr.trim();

            if !stdout.is_empty() && code == 0 {
                emit_chat_lines(tx, stdout);
            } else if !stdout.is_empty() {
                // Ненулевой код, но в result есть текст — это structured error claude
                // (недоступная модель, api-ошибка, лимит: сообщение доезжает в result).
                // Показываем ЯВНО как ошибку с кодом, а не как обычный ответ ⏺ — иначе
                // причина сбоя читается как «ответ модели».
                for line in chat_error_lines(provider.as_str(), code, stdout, lang) {
                    let _ = tx.send(WorkerEvent::Line(line));
                }
            } else if code == 0 {
                let _ = tx.send(WorkerEvent::Line(
                    lang.choose(
                        "Модель не вернула текстовый ответ.",
                        "The model returned no text response.",
                    )
                    .to_string(),
                ));
            } else {
                // Пустой stdout И ненулевой код: обрыв ДО result (сеть/лимит/таймаут).
                // Пустой stderr дал бы немое «no stderr output» — chat_error_lines честно
                // назовёт транзиент и предложит повтор.
                for line in chat_error_lines(provider.as_str(), code, stderr, lang) {
                    let _ = tx.send(WorkerEvent::Line(line));
                }
            }

            if planning {
                let _ = tx.send(WorkerEvent::PlanReady(
                    provider,
                    stdout.to_string(),
                    code,
                    usage,
                ));
            } else {
                let _ = tx.send(WorkerEvent::ChatDone(provider, code, usage));
            }
        }
        Ok(ChatRunResult::Cancelled) => {
            let _ = tx.send(WorkerEvent::Cancelled);
        }
        Err(err) => {
            let _ = tx.send(WorkerEvent::Failed(format!(
                "{}: {}",
                provider_display(provider.as_str(), lang),
                err
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::runs::testkit::*;
    use std::sync::mpsc;

    fn drain(rx: &mpsc::Receiver<WorkerEvent>) -> Vec<WorkerEvent> {
        rx.try_iter().collect()
    }

    // --- Мутант 111:16 (delete `!` в логин-гейте) ---

    #[test]
    fn auth_gate_blocks_and_emits_when_unauthenticated() {
        let (tx, rx) = mpsc::channel();
        let ok = chat_auth_ok(false, Provider::Claude, &tx);
        let events = drain(&rx);
        assert!(!ok);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            WorkerEvent::AuthMissing(Provider::Claude)
        ));
    }

    #[test]
    fn auth_gate_passes_silently_when_authenticated() {
        let (tx, rx) = mpsc::channel();
        let ok = chat_auth_ok(true, Provider::Claude, &tx);
        let events = drain(&rx);
        assert!(ok);
        assert!(events.is_empty());
    }

    // --- Мутант 131:24 (delete `!` в `!stdout.is_empty()`) ---

    #[test]
    fn nonempty_stdout_emits_chat_line_and_done() {
        let (tx, rx) = mpsc::channel();
        emit_chat_run_result(
            Ok(ChatRunResult::Completed(0, "hello".into(), "".into(), None)),
            Provider::Claude,
            false,
            Language::En,
            &tx,
        );
        let events = drain(&rx);
        assert!(events
            .iter()
            .any(|e| matches!(e, WorkerEvent::ChatLine(s) if s.contains("hello"))));
        assert!(events
            .iter()
            .any(|e| matches!(e, WorkerEvent::ChatDone(..))));
    }

    // --- Мутант 133:36 (`==` → `!=` в `code == 0`) ---

    #[test]
    fn empty_stdout_zero_code_emits_no_text_line() {
        let (tx, rx) = mpsc::channel();
        emit_chat_run_result(
            Ok(ChatRunResult::Completed(0, "".into(), "".into(), None)),
            Provider::Claude,
            false,
            Language::En,
            &tx,
        );
        let events = drain(&rx);
        assert!(events.iter().any(
            |e| matches!(e, WorkerEvent::Line(s) if s == "The model returned no text response.")
        ));
        assert!(events
            .iter()
            .any(|e| matches!(e, WorkerEvent::ChatDone(..))));
    }

    // --- Дополнительные ветви ---

    #[test]
    fn nonzero_code_emits_error_lines_and_done() {
        let (tx, rx) = mpsc::channel();
        emit_chat_run_result(
            Ok(ChatRunResult::Completed(1, "".into(), "boom".into(), None)),
            Provider::Claude,
            false,
            Language::En,
            &tx,
        );
        let events = drain(&rx);
        assert!(events
            .iter()
            .any(|e| matches!(e, WorkerEvent::Line(s) if s.contains("boom") || s.contains('1'))));
        assert!(events
            .iter()
            .any(|e| matches!(e, WorkerEvent::ChatDone(_, 1, _))));
    }

    #[test]
    fn nonzero_code_with_stdout_shows_structured_error_not_an_answer() {
        // claude вернул текст в result при коде ≠ 0 (structured error: недоступная модель,
        // лимит, api-ошибка). Показываем как ОШИБКУ с кодом, а не как обычный ответ ⏺.
        let (tx, rx) = mpsc::channel();
        emit_chat_run_result(
            Ok(ChatRunResult::Completed(
                1,
                "There's an issue with the selected model".into(),
                "".into(),
                None,
            )),
            Provider::Claude,
            false,
            Language::En,
            &tx,
        );
        let events = drain(&rx);
        // Заголовок ошибки с кодом + текст причины из result.
        assert!(
            events.iter().any(
                |e| matches!(e, WorkerEvent::Line(s) if s.contains("error") && s.contains('1'))
            ),
            "заголовок ошибки с кодом"
        );
        assert!(
            events.iter().any(|e| matches!(e, WorkerEvent::Line(s)
                if s.contains("issue with the selected model"))),
            "текст ошибки показан"
        );
        // НЕ как обычный ответ: structured error не должен идти через ChatLine («⏺ …»).
        assert!(
            !events.iter().any(|e| matches!(e, WorkerEvent::ChatLine(_))),
            "structured error не должен показываться обычным ответом"
        );
    }

    #[test]
    fn cancelled_result_emits_cancelled() {
        let (tx, rx) = mpsc::channel();
        emit_chat_run_result(
            Ok(ChatRunResult::Cancelled),
            Provider::Claude,
            false,
            Language::En,
            &tx,
        );
        let events = drain(&rx);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], WorkerEvent::Cancelled));
    }

    #[test]
    fn err_result_emits_failed_with_cause() {
        let (tx, rx) = mpsc::channel();
        emit_chat_run_result(
            Err(io::Error::other("oops")),
            Provider::Claude,
            false,
            Language::En,
            &tx,
        );
        let events = drain(&rx);
        assert_eq!(events.len(), 1);
        match &events[0] {
            WorkerEvent::Failed(msg) => assert!(msg.contains("oops")),
            other => panic!("ожидали Failed, получили {other:?}"),
        }
    }

    #[test]
    fn planning_emits_plan_ready_not_chat_done() {
        let (tx, rx) = mpsc::channel();
        emit_chat_run_result(
            Ok(ChatRunResult::Completed(0, "план".into(), "".into(), None)),
            Provider::Claude,
            true,
            Language::En,
            &tx,
        );
        let events = drain(&rx);
        assert!(events
            .iter()
            .any(|e| matches!(e, WorkerEvent::PlanReady(..))));
        assert!(!events
            .iter()
            .any(|e| matches!(e, WorkerEvent::ChatDone(..))));
    }

    // --- Мутант runs.rs:333 (`run_chat_worker_body → ()` И delete `!`): гейт без логина ---
    // Прямой вызов ОБЁРТКИ, а не её внутренностей — иначе мутация тела не ловится.

    #[test]
    fn worker_body_gates_run_and_emits_auth_missing_when_unauthenticated() {
        let (tx, rx) = mpsc::channel();
        run_chat_worker_body(
            false,
            Provider::Claude,
            || panic!("run не должен зваться без логина"),
            false,
            Language::En,
            &tx,
        );
        let events = drain(&rx);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            WorkerEvent::AuthMissing(Provider::Claude)
        ));
    }

    // --- Мутант runs.rs:333 (`run_chat_worker_body → ()` И delete `!`): счастливый путь ---

    #[test]
    fn worker_body_runs_and_emits_result_when_authenticated() {
        let (tx, rx) = mpsc::channel();
        run_chat_worker_body(
            true,
            Provider::Claude,
            || Ok(ChatRunResult::Completed(0, "hi".into(), "".into(), None)),
            false,
            Language::En,
            &tx,
        );
        let events = drain(&rx);
        assert!(events
            .iter()
            .any(|e| matches!(e, WorkerEvent::ChatLine(s) if s.contains("hi"))));
        assert!(events
            .iter()
            .any(|e| matches!(e, WorkerEvent::ChatDone(..))));
    }

    // --- Диспетч start_chat по режиму (delete match arm Plan/Tandem) ---

    #[test]
    fn start_chat_in_plan_mode_dispatches_to_start_plan() {
        let mut app = runs_app();
        app.chat_mode = ChatMode::Plan;
        app.start_chat("t".into());
        // start_plan ставит plan_flow=Planning; при удалении плеча ушло бы в
        // start_chat_with_prompt, и plan_flow остался бы None.
        assert!(matches!(app.plan_flow, PlanFlow::Planning { .. }));
    }

    #[test]
    fn start_chat_in_tandem_mode_dispatches_to_start_tandem() {
        let mut app = runs_app();
        app.chat_mode = ChatMode::Tandem;
        app.start_chat("t".into());
        // start_tandem ставит run_label="Tandem"; при удалении плеча ушло бы в
        // start_chat_with_prompt (run_label = имя провайдера).
        assert_eq!(app.run_label, "Tandem");
    }

    // --- guard process_pending_messages (→() и два ||→&&) ---

    #[test]
    fn process_pending_starts_the_next_queued_message() {
        let mut app = runs_app();
        app.running = false;
        app.pending_messages.clear();
        app.pending_messages.push_back("m".into());
        app.process_pending_messages();
        // →(): очередь не тронута; корректно — сообщение снято и запущено.
        assert!(
            app.pending_messages.is_empty(),
            "сообщение снято из очереди"
        );
        assert!(app.running, "и запущено (running=true)");
    }

    #[test]
    fn process_pending_does_nothing_while_running() {
        let mut app = runs_app();
        app.running = true; // первый ||: running обязан заблокировать
        app.pending_messages.clear();
        app.pending_messages.push_back("m".into());
        let before = app.transcript.len();
        app.process_pending_messages();
        // ||→&& (running && plan_gate): guard стал бы false → снял бы из очереди и
        // start_chat при running=true до-эхнул бы «в очереди» в ленту.
        assert_eq!(
            app.transcript.len(),
            before,
            "при running=true очередь не трогается"
        );
    }

    #[test]
    fn process_pending_does_nothing_while_the_plan_gate_is_open() {
        let mut app = runs_app();
        app.running = false;
        app.pending_plan = Some(PendingPlan {
            task: "t".into(),
            plan: "p".into(),
        });
        assert!(app.plan_gate_active());
        app.pending_messages.clear();
        app.pending_messages.push_back("m".into());
        app.process_pending_messages();
        // ||→&& (plan_gate && ask): при ask=false guard стал бы false → снял бы очередь
        // и стартовал ран. Корректно — гейт плана блокирует.
        assert!(!app.running, "гейт плана открыт → ран не стартует");
        assert_eq!(app.pending_messages.len(), 1, "очередь не тронута");
    }
}
