use super::*;

impl App {
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

        (self.run_hooks.spawn)(
            self.tx.clone(),
            Box::new(move || {
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
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::runs::testkit::*;

    // --- guard start_task (delete `!ensure_auth_ready`) ---

    #[test]
    fn start_task_bails_and_opens_auth_when_not_authenticated() {
        let mut app = runs_app();
        app.mode = Mode::ClaudeCodex; // режим требует обоих провайдеров
        app.run_hooks.authenticated = |_| false; // не залогинен
        app.start_task("t".into());
        // Корректно: `!ensure_auth_ready` → ранний return, открыт экран авторизации.
        // Мутант (delete `!`) полез бы к движку (onboarding остался бы None).
        assert!(app.onboarding.is_some(), "не залогинен → экран авторизации");
        assert!(!app.running, "ран не стартует без логина");
    }
}
