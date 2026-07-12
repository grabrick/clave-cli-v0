use super::*;

impl App {
    pub(crate) fn resolved_work_dir(&self) -> PathBuf {
        resolve_work_dir(&self.work_dir, &launch_work_dir())
    }

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
        self.dev_run = false;
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
        spawn_worker(self.tx.clone(), move || {
            // Логин проверяем здесь, в воркере (не морозя UI). Не залогинен → событие.
            if !provider_authenticated(provider) {
                let _ = tx.send(WorkerEvent::AuthMissing(provider));
                return;
            }
            let command_result = run_chat_provider(
                provider.as_str(),
                &effort,
                &prompt,
                &work_dir,
                cancel_rx,
                tx.clone(),
                lang,
                access,
            );

            match command_result {
                Ok(ChatRunResult::Completed(code, stdout, stderr, usage)) => {
                    let stdout = stdout.trim();
                    let stderr = stderr.trim();

                    if !stdout.is_empty() {
                        emit_chat_lines(&tx, stdout);
                    } else if code == 0 {
                        let _ = tx.send(WorkerEvent::Line(
                            lang.choose(
                                "Модель не вернула текстовый ответ.",
                                "The model returned no text response.",
                            )
                            .to_string(),
                        ));
                    } else {
                        // Показываем КОД выхода и причину — раньше код терялся, а пустой
                        // stderr давал немое «no stderr output» (см. chat_error_lines).
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
        });
    }

    pub(crate) fn start_task(&mut self, task: String) {
        if self.running {
            self.push_system(
                self.lang
                    .choose("Clave уже выполняется.", "Clave is already running."),
            );
            return;
        }

        if !self.ensure_auth_ready_for_current_mode() {
            return;
        }

        let engine = match engine_path() {
            Some(path) => path,
            None => {
                self.status = "engine missing".to_string();
                self.push_system(self.lang.choose(
                    "spec-clave не найден. Задай CLAVE_ENGINE или запусти из корня проекта.",
                    "spec-clave engine not found. Set CLAVE_ENGINE or run from project root.",
                ));
                return;
            }
        };
        let (cancel_tx, cancel_rx) = mpsc::channel();

        self.set_chat_title_from_prompt_if_needed(&task);

        self.running = true;
        self.dev_run = false;
        self.run_started_at = Some(Instant::now());
        self.last_run_duration = None;
        self.run_label = ENGINE_NAME.to_string();
        self.run_token_estimate = Some(estimate_tokens(&task));
        self.run_activity.clear();
        self.cancel_tx = Some(cancel_tx);
        self.last_ctrl_c_at = None;
        self.status = self.lang.choose("запущено", "running").to_string();
        self.push_system(format!("◆ {task}"));
        self.push_system(format!(
            "{} {} {} {} · effort {}.",
            self.lang.choose("⏺ Запускаю режим", "⏺ Running"),
            self.mode.as_str(),
            self.lang.choose("на раундов:", "with round(s):"),
            self.rounds,
            self.effort_summary()
        ));

        let tx = self.tx.clone();
        let mode = self.mode;
        let rounds = self.rounds.to_string();
        let out_dir = self.out_dir.clone();
        let common_effort = effort_label(self.effort_index).to_string();
        let architect_provider = mode.architect_provider();
        let reviewer_provider = mode.reviewer_provider();
        let architect_effort = self
            .provider_effort(architect_provider.as_str())
            .to_string();
        let reviewer_effort = self.provider_effort(reviewer_provider.as_str()).to_string();
        let work_dir = self.resolved_work_dir();
        let work_dir_arg = work_dir.to_string_lossy().to_string();
        self.push_run_activity(format!(
            "{} {}",
            self.lang.choose("инструмент:", "tool:"),
            ENGINE_NAME
        ));
        self.push_run_activity(format!(
            "{} {}",
            self.lang.choose("cwd:", "cwd:"),
            work_dir.display()
        ));
        self.push_run_activity(format!(
            "{} {} · {} {}",
            self.lang.choose("исполнитель:", "executor:"),
            architect_provider.as_str(),
            self.lang.choose("ревьюер:", "reviewer:"),
            reviewer_provider.as_str()
        ));
        self.push_run_activity(format!(
            "{} {} · {} {} · out {}",
            self.lang.choose("effort:", "effort:"),
            self.effort_summary(),
            self.lang.choose("раунды:", "rounds:"),
            self.rounds,
            self.out_dir
        ));

        spawn_worker(self.tx.clone(), move || {
            let mut args = Vec::new();

            match mode {
                Mode::CodexOnly => args.push("--codex-only".to_string()),
                Mode::ClaudeOnly => {
                    args.extend([
                        "--architect".to_string(),
                        "claude".to_string(),
                        "--reviewer".to_string(),
                        "claude".to_string(),
                    ]);
                }
                Mode::ClaudeCodex => {
                    args.extend([
                        "--architect".to_string(),
                        "claude".to_string(),
                        "--reviewer".to_string(),
                        "codex".to_string(),
                    ]);
                }
                Mode::CodexClaude => {
                    args.extend([
                        "--architect".to_string(),
                        "codex".to_string(),
                        "--reviewer".to_string(),
                        "claude".to_string(),
                    ]);
                }
            }

            args.extend([
                "--cwd".to_string(),
                work_dir_arg,
                "--rounds".to_string(),
                rounds,
                "--out".to_string(),
                out_dir,
                "--effort".to_string(),
                common_effort,
                "--architect-effort".to_string(),
                architect_effort,
                "--reviewer-effort".to_string(),
                reviewer_effort,
                task,
            ]);

            let mut engine_command = Command::new(&engine);
            engine_command
                .current_dir(&work_dir)
                .args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            configure_process_group(&mut engine_command);
            let mut child = match engine_command.spawn() {
                Ok(child) => child,
                Err(err) => {
                    let _ = tx.send(WorkerEvent::Failed(format!(
                        "Failed to spawn {}: {err}",
                        engine.display()
                    )));
                    return;
                }
            };

            if let Some(stdout) = child.stdout.take() {
                spawn_reader(stdout, tx.clone());
            }

            if let Some(stderr) = child.stderr.take() {
                spawn_reader(stderr, tx.clone());
            }

            loop {
                if cancel_rx.try_recv().is_ok() {
                    // Убиваем всю группу движка (spec-clave + порождённые им claude/codex).
                    kill_process_tree(&mut child);
                    let _ = tx.send(WorkerEvent::Cancelled);
                    return;
                }

                match child.try_wait() {
                    Ok(Some(status)) => {
                        let _ = tx.send(WorkerEvent::Done(status.code().unwrap_or(1)));
                        return;
                    }
                    Ok(None) => thread::sleep(Duration::from_millis(80)),
                    Err(err) => {
                        let _ = tx.send(WorkerEvent::Failed(format!("Wait failed: {err}")));
                        return;
                    }
                }
            }
        });
    }
}
