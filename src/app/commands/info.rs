use super::*;

impl App {
    pub(crate) fn show_status(&mut self) {
        self.status = self.lang.choose("статус", "status").to_string();
        self.push_system(self.lang.choose("⏺ Статус сессии", "⏺ Session status"));
        self.push_status_row(
            self.lang.choose("Режим", "Mode"),
            self.mode.as_str().to_string(),
        );
        self.push_status_row(
            self.lang.choose("Исполнитель", "Executor"),
            self.mode.architect_provider().title().to_string(),
        );
        self.push_status_row(
            self.lang.choose("Ревьюер", "Reviewer"),
            self.mode.reviewer_provider().title().to_string(),
        );
        self.push_status_row(
            self.lang.choose("Простой чат", "Direct chat"),
            self.direct_provider.title().to_string(),
        );
        self.push_status_row(self.lang.choose("Effort", "Effort"), self.effort_summary());
        self.push_status_row(
            self.lang.choose("Раунды", "Rounds"),
            self.rounds.to_string(),
        );
        self.push_status_row(
            self.lang.choose("Язык", "Language"),
            self.lang.as_str().to_string(),
        );
        self.push_status_row(
            self.lang.choose("Тема", "Theme"),
            self.theme.title().to_string(),
        );
        self.push_status_row(
            self.lang.choose("Рабочая директория", "Working directory"),
            self.resolved_work_dir().display().to_string(),
        );
        self.push_status_row(
            self.lang.choose("Артефакты", "Artifacts"),
            self.out_dir.clone(),
        );
        self.push_status_row(self.lang.choose("Чат", "Chat"), self.chat_id.clone());
    }

    pub(crate) fn show_version(&mut self) {
        let mark = |present: bool| if present { "✓" } else { "✗" };
        self.push_system(format!("⏺ {APP_COMMAND} v{}", env!("CARGO_PKG_VERSION")));
        self.push_system(format!(
            "  {} · {}",
            env!("CARGO_PKG_REPOSITORY"),
            env!("CARGO_PKG_LICENSE")
        ));
        self.push_system(format!(
            "  claude {}  codex {}",
            mark(provider_binary_present("claude")),
            mark(provider_binary_present("codex")),
        ));
        self.push_system(format!(
            "  {}: {}",
            self.lang.choose("режим", "mode"),
            self.mode.as_str()
        ));
        self.push_system(format!(
            "  {}: {}",
            self.lang.choose("состояние", "state"),
            clave_state_dir().display()
        ));
        self.status = self.lang.choose("версия", "version").to_string();
    }

    pub(crate) fn show_cost(&mut self) {
        self.status = self.lang.choose("расход", "cost").to_string();
        self.push_system(self.lang.choose("⏺ Расход сессии", "⏺ Session cost"));

        let claude = self.usage.claude;
        let codex = self.usage.codex;
        let total_tokens = self.usage.total_tokens();
        let total_cost = self.usage.total_cost_usd();
        let minutes = self.usage.started_at.elapsed().as_secs() / 60;

        if claude.requests == 0 && codex.requests == 0 {
            self.push_status_row(
                self.lang.choose("Данные", "Data"),
                self.lang
                    .choose("пока нет запросов", "no requests yet")
                    .to_string(),
            );
            return;
        }

        let req = self.lang.choose("запр.", "req");
        if claude.requests > 0 {
            self.push_status_row(
                "Claude",
                format!(
                    "{} {req} · {} in · {} out · ${:.4}",
                    claude.requests,
                    format_token_count(claude.total.input as usize),
                    format_token_count(claude.total.output as usize),
                    claude.total.cost_usd,
                ),
            );
        }
        if codex.requests > 0 {
            self.push_status_row(
                "Codex",
                format!(
                    "{} {req} · {} tok · $—",
                    codex.requests,
                    format_token_count(codex.total.tokens() as usize),
                ),
            );
        }
        self.push_status_row(
            self.lang.choose("Итого", "Total"),
            format!(
                "≈ {} {} · ${:.4}",
                format_token_count(total_tokens as usize),
                self.lang.choose("токенов", "tokens"),
                total_cost,
            ),
        );
        self.push_status_row(
            self.lang.choose("Сессия", "Session"),
            format!(
                "{minutes} {} · {}",
                self.lang.choose("мин", "min"),
                self.lang.choose(
                    "read-only chat, инструменты отключены",
                    "read-only chat, tools disabled"
                ),
            ),
        );
    }

    pub(crate) fn show_uptime(&mut self) {
        self.status = self.lang.choose("аптайм", "uptime").to_string();
        self.push_system(
            self.lang
                .choose("⏺ Время работы сессии", "⏺ Session uptime"),
        );
        self.push_status_row(
            self.lang.choose("Работает", "Running"),
            format_elapsed(self.usage.started_at.elapsed()),
        );
    }

    pub(crate) fn export_chat(&mut self) {
        let dir = self.resolved_work_dir();
        let path = dir.join(format!("clave-{}.md", sanitize_chat_id(&self.chat_id)));
        let content = format!(
            "# Clave · {}\n\n{}\n",
            self.chat_id,
            self.transcript.join("\n")
        );
        match fs::write(&path, content) {
            Ok(()) => self.push_system(format!(
                "{} {}",
                self.lang.choose("Чат экспортирован:", "Chat exported:"),
                path.display()
            )),
            Err(err) => self.push_system(format!(
                "{} {}",
                self.lang
                    .choose("Не удалось экспортировать:", "Export failed:"),
                err
            )),
        }
    }

    fn push_status_row(&mut self, label: &str, value: String) {
        self.push_system(format!("  ⎿ {label}: {value}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::testkit::*;

    /// /status, /version, /uptime и /export печатают конкретные строки — молчаливая
    /// заглушка вместо них означала бы «команда есть, а ответа нет».
    #[test]
    fn info_commands_report_session_facts() {
        let (mut app, dir) = app_for_commands();

        let status = joined(&mut app, "/status");
        assert!(status.contains("⏺ Статус сессии"));
        assert!(status.contains(&format!("  ⎿ Режим: {}", app.mode.as_str())));
        assert!(status.contains(&format!("  ⎿ Раунды: {}", app.rounds)));
        assert!(status.contains(&format!("  ⎿ Чат: {}", app.chat_id)));
        assert_eq!(app.status, "статус");

        let version = joined(&mut app, "/version");
        assert!(version.contains(&format!("⏺ {APP_COMMAND} v{}", env!("CARGO_PKG_VERSION"))));
        assert!(version.contains("claude "));
        assert_eq!(app.status, "версия");

        let uptime = joined(&mut app, "/uptime");
        assert!(uptime.contains("⏺ Время работы сессии"));
        assert!(uptime.contains("  ⎿ Работает: "));
        assert_eq!(app.status, "аптайм");

        let export = joined(&mut app, "/export");
        assert!(export.contains("Чат экспортирован:"));
        let exported = dir.join(format!("clave-{}.md", app.chat_id));
        let content = fs::read_to_string(&exported).expect("экспорт обязан лечь на диск");
        assert!(content.contains(&app.chat_id));

        let _ = fs::remove_dir_all(&dir);
    }

    /// /cost: без запросов — честное «пока нет запросов»; с запросами — только те
    /// провайдеры, что реально работали, и минуты сессии (а не секунды и не остаток).
    #[test]
    fn cost_reports_only_providers_that_ran() {
        let (mut app, dir) = app_for_commands();

        let empty = joined(&mut app, "/cost");
        assert!(empty.contains("⏺ Расход сессии"));
        assert!(empty.contains("  ⎿ Данные: пока нет запросов"));
        assert!(!empty.contains("Claude:") && !empty.contains("Codex:"));
        assert_eq!(app.status, "расход");

        app.usage.started_at = Instant::now() - Duration::from_secs(300);
        app.usage.claude.requests = 2;
        app.usage.claude.total = RunUsage {
            input: 1000,
            output: 500,
            cost_usd: 0.25,
            ..RunUsage::default()
        };
        let claude_only = joined(&mut app, "/cost");
        assert!(claude_only.contains("  ⎿ Claude: 2 запр."));
        assert!(claude_only.contains("$0.2500"));
        assert!(
            !claude_only.contains("  ⎿ Codex:"),
            "Codex без запросов показывать нельзя: {claude_only}"
        );
        assert!(claude_only.contains("  ⎿ Итого: "));
        assert!(
            claude_only.contains("  ⎿ Сессия: 5 мин"),
            "минуты сессии считаются как секунды/60: {claude_only}"
        );
        assert!(!claude_only.contains("пока нет запросов"));

        let (mut app, _) = app_for_commands();
        app.usage.codex.requests = 3;
        app.usage.codex.total = RunUsage {
            input: 700,
            output: 300,
            ..RunUsage::default()
        };
        let codex_only = joined(&mut app, "/cost");
        assert!(codex_only.contains("  ⎿ Codex: 3 запр."));
        assert!(
            !codex_only.contains("  ⎿ Claude:"),
            "Claude без запросов показывать нельзя: {codex_only}"
        );
        assert!(!codex_only.contains("пока нет запросов"));

        let _ = fs::remove_dir_all(&dir);
    }
}
