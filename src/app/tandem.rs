use super::*;

impl App {
    /// Активен ли гейт тандема «нет консенсуса»: воркер жив и ждёт решения. В отличие от
    /// plan-гейта (`plan_gate_active`), здесь `running` остаётся true — воркер заблокирован,
    /// а не завершён.
    pub(crate) fn tandem_gate_active(&self) -> bool {
        self.tandem_gate
    }

    /// Enter на гейте: исполнить последнюю версию. Разблокирует воркер — тот идёт в фазу
    /// исполнения. `take` канала гасит гейт и исключает повторную отправку.
    pub(crate) fn tandem_gate_approve(&mut self) {
        if let Some(tx) = self.tandem_gate_tx.take() {
            let _ = tx.send(TandemGate::Execute);
        }
        self.tandem_gate = false;
        self.status = self.lang.choose("исполняю...", "executing...").to_string();
        self.push_system(self.lang.choose(
            "▶ Исполняю последнюю версию.",
            "▶ Executing the latest version.",
        ));
    }

    /// Esc на гейте: отменить, файлы не тронуты. Разблокирует воркер — тот вернёт
    /// `Cancelled`, а его обработчик доснимет состояние прогона.
    pub(crate) fn tandem_gate_abort(&mut self) {
        if let Some(tx) = self.tandem_gate_tx.take() {
            let _ = tx.send(TandemGate::Abort);
        }
        self.tandem_gate = false;
    }

    /// Запустить тандем: исполнитель (architect) + критик (reviewer) из текущего Mode.
    pub(crate) fn start_tandem(&mut self, task: String) {
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

        let executor = self.mode.architect_provider();
        let critic = self.mode.reviewer_provider();
        if executor == critic {
            self.push_system(self.lang.choose(
                "⚠ Тандем эффективнее с разными моделями — смени роли через /mode.",
                "⚠ Tandem works best with two different models — change roles via /mode.",
            ));
        }

        let executor_effort = self.provider_effort(executor.as_str()).to_string();
        let critic_effort = self.provider_effort(critic.as_str()).to_string();
        let rounds = self.rounds;
        let lang = self.lang;
        let work_dir = self.resolved_work_dir();
        let task_run = task.clone();
        let (cancel_tx, cancel_rx) = mpsc::channel();
        // Отдельный канал решения на гейте «нет консенсуса»: воркер блокируется на нём,
        // UI отвечает Execute/Abort из handle_input_key.
        let (gate_tx, gate_rx) = mpsc::channel();

        self.set_chat_title_from_prompt_if_needed(&task);

        self.running = true;
        self.run_started_at = Some(Instant::now());
        self.last_run_duration = None;
        self.run_label = self.lang.choose("Тандем", "Tandem").to_string();
        self.run_token_estimate = Some(estimate_tokens(&task));
        self.run_activity.clear();
        self.cancel_tx = Some(cancel_tx);
        self.tandem_gate = false;
        self.tandem_gate_tx = Some(gate_tx);
        self.last_ctrl_c_at = None;
        self.status = self.lang.choose("тандем...", "tandem...").to_string();
        self.push_system(format!("◆ {task}"));
        self.push_run_activity(format!(
            "{} {} · {} {}",
            self.lang.choose("исполнитель:", "executor:"),
            executor.as_str(),
            self.lang.choose("критик:", "critic:"),
            critic.as_str()
        ));
        self.push_run_activity(format!(
            "{} {}",
            self.lang.choose("cwd:", "cwd:"),
            work_dir.display()
        ));
        self.push_run_activity(format!(
            "{} {}",
            self.lang.choose("раунды дебатов:", "debate rounds:"),
            rounds
        ));

        let tx = self.tx.clone();
        (self.run_hooks.spawn)(
            self.tx.clone(),
            Box::new(move || {
                let result = run_tandem(
                    executor.as_str(),
                    critic.as_str(),
                    &executor_effort,
                    &critic_effort,
                    &task_run,
                    rounds,
                    &work_dir,
                    cancel_rx,
                    gate_rx,
                    tx.clone(),
                    lang,
                );
                match result {
                    Ok(TandemResult::Completed(code, usage)) => {
                        let _ = tx.send(WorkerEvent::ChatDone(executor, code, usage));
                    }
                    Ok(TandemResult::Cancelled) => {
                        let _ = tx.send(WorkerEvent::Cancelled);
                    }
                    Err(err) => {
                        let _ = tx.send(WorkerEvent::Failed(format!(
                            "{}: {}",
                            lang.choose("Тандем", "Tandem"),
                            err
                        )));
                    }
                }
            }),
        );
    }
}
