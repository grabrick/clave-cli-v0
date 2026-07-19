use super::*;

impl App {
    pub(crate) fn run_planning_preset(
        &mut self,
        title_ru: &'static str,
        title_en: &'static str,
        rest: &str,
        fallback_ru: &'static str,
        fallback_en: &'static str,
    ) {
        let focus = if rest.trim().is_empty() {
            self.lang.choose(fallback_ru, fallback_en).to_string()
        } else {
            rest.trim().to_string()
        };
        let task = format!("{}:\n{}", self.lang.choose(title_ru, title_en), focus);
        self.start_task(task);
    }

    pub(crate) fn run_advisor_command(&mut self, rest: &str) {
        let prompt = if rest.trim().is_empty() {
            self.lang
                .choose(
                    "Оцени текущий контекст как технический советник: что я упускаю, какой следующий шаг самый разумный, какие риски проверить?",
                    "Review the current context as a technical advisor: what am I missing, what is the smartest next step, and which risks should be checked?",
                )
                .to_string()
        } else {
            format!(
                "{}\n{}",
                self.lang.choose(
                    "Ответь как технический советник. Дай ясную рекомендацию без запуска planning-loop:",
                    "Answer as a technical advisor. Give a clear recommendation without running the planning loop:",
                ),
                rest.trim()
            )
        };
        let display = if rest.trim().is_empty() {
            "/advisor".to_string()
        } else {
            format!("/advisor {}", rest.trim())
        };
        self.start_chat_with_prompt(display, prompt);
    }

    pub(crate) fn run_btw_command(&mut self, rest: &str) {
        if rest.trim().is_empty() {
            self.push_system(
                self.lang
                    .choose("Использование: /btw <вопрос>", "Usage: /btw <question>"),
            );
            return;
        }

        let prompt = format!(
            "{}\n{}",
            self.lang.choose(
                "Ответь на быстрый побочный вопрос, не меняя план и не трогая файлы:",
                "Answer this quick side question without changing the plan or touching files:",
            ),
            rest.trim()
        );
        self.start_chat_with_prompt(format!("/btw {}", rest.trim()), prompt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::testkit::*;

    const BUSY: &str = "Clave уже выполняется.";

    /// Запускающие команды при занятом приложении отвечают busy-преflight, а не
    /// «неизвестная команда»: так каждая ветка проверяется без спавна провайдера.
    #[test]
    fn launching_commands_reach_their_runners() {
        let (mut app, dir) = app_for_commands();

        for command in [
            "/plan задача",
            "/clave задача",
            "/dev задача",
            "/brainstorm",
            "/blueprint",
            "/finish-branch",
            "/split-work",
            "/worktrees",
            "/autofix-pr",
            "/advisor",
            "/advisor как быть",
            "/btw вопрос",
        ] {
            let out = joined(&mut app, command);
            assert!(
                out.contains(BUSY),
                "{command} обязана дойти до запуска (busy-преflight), получено: {out}"
            );
        }

        assert!(joined(&mut app, "/plan").contains("Использование: /plan <задача>"));
        assert!(joined(&mut app, "/dev").contains("Использование: /dev <задача>"));
        assert!(joined(&mut app, "/btw").contains("Использование: /btw <вопрос>"));

        assert!(joined(&mut app, "/retry").contains("Нет последнего запроса для повтора."));
        app.last_chat_message = Some("прошлый запрос".to_string());
        assert!(
            joined(&mut app, "/retry").contains("в очереди: прошлый запрос"),
            "повтор обязан отправить прошлое сообщение"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
