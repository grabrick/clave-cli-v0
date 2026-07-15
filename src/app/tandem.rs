use super::*;

impl App {
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

        self.set_chat_title_from_prompt_if_needed(&task);

        self.running = true;
        self.run_started_at = Some(Instant::now());
        self.last_run_duration = None;
        self.run_label = self.lang.choose("Тандем", "Tandem").to_string();
        self.run_token_estimate = Some(estimate_tokens(&task));
        self.run_activity.clear();
        self.cancel_tx = Some(cancel_tx);
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
