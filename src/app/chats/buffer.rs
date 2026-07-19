use super::*;

impl App {
    pub(crate) fn remember_history_entry(&mut self, line: &str) {
        self.history.retain(|entry| entry != line);
        self.history.push(line.to_string());
        if self.history.len() > MAX_HISTORY_LINES {
            let remove_count = self.history.len() - MAX_HISTORY_LINES;
            self.history.drain(0..remove_count);
        }

        if let Err(err) = save_history(&self.history_path, &self.history) {
            self.status = self
                .lang
                .choose("ошибка истории", "history error")
                .to_string();
            self.transcript.push(format!(
                "{} {}",
                self.lang
                    .choose("Не удалось сохранить историю:", "Failed to save history:"),
                err
            ));
        }
    }

    pub(crate) fn push_system(&mut self, line: impl Into<String>) {
        let line = line.into();
        if let Err(err) = append_chat_line(&self.chat_path, &line) {
            self.status = self.lang.choose("ошибка чата", "chat error").to_string();
            self.transcript.push(format!(
                "{} {}",
                self.lang
                    .choose("Не удалось сохранить чат:", "Failed to save chat:"),
                err
            ));
        }

        // Строка добавляется только в transcript; нижний viewport покажет её в
        // хвосте, а runtime::flush_overflow вытеснит старое в скроллбэк по мере
        // надобности (append-only история).
        self.transcript.push(line);
        if self.transcript.len() > MAX_TRANSCRIPT_LINES {
            let remove_count = self.transcript.len() - MAX_TRANSCRIPT_LINES;
            self.transcript.drain(0..remove_count);
            // Срезанные строки были из уже вытесненной «головы» — сдвигаем границу.
            self.scrollback_count = self.scrollback_count.saturating_sub(remove_count);
        }
    }

    /// Сбрасывает границу вытеснения: вызывать при ПОЛНОЙ замене transcript
    /// (новый чат, /resume, /clear) — содержимое сменилось, прошлая «голова»
    /// больше не относится к текущей ленте.
    pub(crate) fn reset_scrollback(&mut self) {
        self.scrollback_count = 0;
        self.flush_state = TranscriptRenderState::default();
        // Уже напечатанную историю из нативного скроллбэка иначе не убрать —
        // просим рендер полностью очистить терминал (экран + скроллбэк).
        self.pending_clear_screen = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::chats::testkit::*;

    /// История ввода: дубликаты всплывают наверх, длина держится в пределах лимита.
    #[test]
    fn remember_history_entry_dedupes_and_trims_to_the_limit() {
        let (mut app, dir) = app_for_chats();
        app.history = vec!["a".to_string(), "b".to_string()];

        app.remember_history_entry("b");
        assert_eq!(app.history, vec!["a".to_string(), "b".to_string()]);

        app.history = (0..MAX_HISTORY_LINES + 5)
            .map(|i| format!("line-{i}"))
            .collect();
        app.remember_history_entry("свежая строка");

        assert_eq!(app.history.len(), MAX_HISTORY_LINES);
        assert_eq!(
            app.history[0], "line-6",
            "обрезать нужно ровно лишнее с головы"
        );
        assert_eq!(app.history.last().unwrap(), "свежая строка");

        // Граница: ровно MAX_HISTORY_LINES после вставки — резать нечего.
        app.history = (0..MAX_HISTORY_LINES - 1)
            .map(|i| format!("line-{i}"))
            .collect();
        app.remember_history_entry("ровно на границе");

        assert_eq!(app.history.len(), MAX_HISTORY_LINES);
        assert_eq!(
            app.history[0], "line-0",
            "на границе голова истории остаётся нетронутой"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// push_system дописывает строку в файл чата и держит ленту в пределах лимита,
    /// сдвигая границу вытеснения ровно на число срезанных строк.
    #[test]
    fn push_system_appends_to_disk_and_trims_the_transcript() {
        let (mut app, dir) = app_for_chats();
        app.transcript = (0..MAX_TRANSCRIPT_LINES + 5)
            .map(|i| format!("line-{i}"))
            .collect();
        app.scrollback_count = 10;

        app.push_system("хвост");

        assert_eq!(app.transcript.len(), MAX_TRANSCRIPT_LINES);
        assert_eq!(app.transcript[0], "line-6");
        assert_eq!(app.transcript.last().unwrap(), "хвост");
        assert_eq!(app.scrollback_count, 4);

        let saved = load_chat_transcript(&app.chat_path).expect("load chat");
        assert!(
            saved.iter().any(|line| line == "хвост"),
            "строка обязана уехать в файл чата"
        );

        // Граница: ровно MAX_TRANSCRIPT_LINES после вставки — ни строки не срезано,
        // граница вытеснения не сдвигается.
        app.transcript = (0..MAX_TRANSCRIPT_LINES - 1)
            .map(|i| format!("line-{i}"))
            .collect();
        app.scrollback_count = 10;
        app.push_system("ровно на границе");

        assert_eq!(app.transcript.len(), MAX_TRANSCRIPT_LINES);
        assert_eq!(app.transcript[0], "line-0");
        assert_eq!(app.scrollback_count, 10);

        // reset_scrollback: граница обнуляется, экран просят перерисовать целиком.
        app.pending_clear_screen = false;
        app.scrollback_count = 5;
        app.reset_scrollback();
        assert_eq!(app.scrollback_count, 0);
        assert!(app.pending_clear_screen);

        let _ = fs::remove_dir_all(&dir);
    }
}
