use super::*;

impl App {
    pub(crate) fn push_run_activity(&mut self, activity: impl Into<String>) {
        let activity = activity.into();
        if activity.trim().is_empty() {
            return;
        }

        self.run_activity.push_back(activity);
        while self.run_activity.len() > 5 {
            self.run_activity.pop_front();
        }
    }

    pub(crate) fn record_worker_activity(&mut self, line: &str) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return;
        }

        if let Some(path) = trimmed.strip_prefix("Final brief: ") {
            self.push_run_activity(format!(
                "{} {}",
                self.lang.choose("итог:", "final:"),
                truncate_chars(path, 96)
            ));
        } else {
            self.push_run_activity(truncate_chars(trimmed, 120));
        }
    }

    pub(crate) fn push_final_brief(&mut self, path: &str) {
        match final_brief_lines_for_chat(path, self.lang) {
            Ok(lines) => {
                self.push_system(self.lang.choose("⏺ Итоговый ответ", "⏺ Final answer"));
                for line in lines {
                    self.push_system(line);
                }
            }
            Err(err) => self.push_system(format!(
                "{} {}",
                self.lang.choose(
                    "Не удалось прочитать итоговый ответ:",
                    "Failed to read final answer:"
                ),
                err
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::events::testkit::*;

    /// Лента активности держит ровно пять последних строк — не больше и не меньше.
    #[test]
    fn run_activity_keeps_the_last_five_lines_and_skips_blanks() {
        let (mut app, _dir) = app_for_events();
        for i in 0..7 {
            app.push_run_activity(format!("шаг {i}"));
        }

        let lines: Vec<String> = app.run_activity.iter().cloned().collect();
        assert_eq!(lines, vec!["шаг 2", "шаг 3", "шаг 4", "шаг 5", "шаг 6"]);

        app.push_run_activity("   ");
        assert_eq!(app.run_activity.len(), 5, "пустая строка не добавляется");
    }

    /// Строка воркера про итоговый файл показывается как «итог: …», обычная — как есть.
    #[test]
    fn worker_activity_marks_the_final_brief_and_ignores_blanks() {
        let (mut app, _dir) = app_for_events();
        app.record_worker_activity("Final brief: /tmp/run/brief.md");
        assert_eq!(
            app.run_activity.back().map(String::as_str),
            Some("итог: /tmp/run/brief.md")
        );

        app.record_worker_activity("  правлю модуль  ");
        assert_eq!(
            app.run_activity.back().map(String::as_str),
            Some("правлю модуль")
        );

        app.record_worker_activity("   ");
        assert_eq!(app.run_activity.len(), 2, "пустая строка не пишется");
    }

    /// Итоговая сводка обязана появиться в ленте — с заголовком и содержимым.
    #[test]
    fn final_brief_is_pushed_into_the_transcript() {
        let (mut app, dir) = app_for_events();
        let brief = dir.join("brief.md");
        fs::write(&brief, "## Current Spec\nсделать X\n").expect("write brief");

        app.push_final_brief(&brief.to_string_lossy());

        assert!(
            app.transcript.iter().any(|line| line == "⏺ Итоговый ответ"),
            "заголовок сводки: {:?}",
            app.transcript
        );
        assert!(app.transcript.iter().any(|line| line == "## Текущая спека"));
        assert!(app.transcript.iter().any(|line| line == "сделать X"));
    }

    #[test]
    fn unreadable_final_brief_is_reported_in_the_transcript() {
        let (mut app, dir) = app_for_events();

        app.push_final_brief(&dir.join("нет-такого.md").to_string_lossy());

        assert!(
            app.transcript
                .last()
                .is_some_and(|line| line.contains("Не удалось прочитать итоговый ответ")),
            "ошибку чтения показываем: {:?}",
            app.transcript
        );
    }
}
