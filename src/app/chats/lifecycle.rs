use super::*;

impl App {
    pub(crate) fn start_new_chat(&mut self) {
        self.reset_to_new_chat();
        self.push_command_result(format!(
            "{} {}",
            self.lang.choose("Новый чат:", "New chat:"),
            self.chat_id
        ));
    }

    /// `/clear` как в Claude: удаляет ТЕКУЩИЙ (именованный) чат целиком и начинает
    /// свежий пустой контекст. Имя хранится в самом файле чата, поэтому remove_file
    /// уносит и его. Удаляем ДО создания нового (reset меняет chat_path).
    pub(crate) fn clear_current_chat(&mut self) {
        let _ = fs::remove_file(&self.chat_path);
        self.reset_to_new_chat();
        // /clear = чистый старт: показываем welcome (как при запуске), но в файл его
        // НЕ пишем (не через push_system) → файл пуст → следующее окно тоже даст
        // welcome. Раньше тут был push_command_result — он сохранялся и забивал welcome.
        let welcome = crate::runtime::welcome_lines(self);
        self.transcript = welcome;
        self.status = self
            .lang
            .choose("контекст очищен", "context cleared")
            .to_string();
    }

    /// Сброс к свежему пустому чату: новый id, имя по умолчанию, пустая лента, чистый
    /// экран и сброшенный лоадер. Без сообщения — его шлёт вызывающий.
    fn reset_to_new_chat(&mut self) {
        self.chat_id = new_chat_id();
        self.chat_path = chat_path_for_id(&self.chats_dir, &self.chat_id);
        self.chat_title = self.chat_id.clone();
        self.chat_title_custom = false;
        self.transcript.clear();
        self.reset_scrollback();
        self.last_run = None;
        self.last_run_duration = None;
        self.pending_plan = None;
        self.plan_flow = PlanFlow::None;
        self.status = self.lang.choose("новый чат", "new chat").to_string();

        if let Err(err) = save_chat_transcript(&self.chat_path, &self.chat_id, &self.transcript) {
            self.transcript.push(format!(
                "{} {}",
                self.lang.choose(
                    "Не удалось создать файл чата:",
                    "Failed to create chat file:"
                ),
                err
            ));
        }

        self.save_current_config(true);
    }

    pub(crate) fn resume_chat(&mut self, chat_id: &str) {
        let chat_id = sanitize_chat_id(chat_id);
        if chat_id.is_empty() {
            self.push_command_result(self.lang.choose(
                "Использование: /resume <id-чата>",
                "Usage: /resume <chat-id>",
            ));
            return;
        }

        let Some(path) = existing_chat_path(&self.chats_dir, &chat_id) else {
            self.push_command_result(self.lang.choose("Чат не найден.", "Chat not found."));
            return;
        };
        match load_chat_transcript(&path) {
            Ok(lines) if !lines.is_empty() => {
                self.chat_id = chat_id;
                self.chat_path = path;
                self.transcript = lines;
                self.refresh_current_chat_title();
                self.reset_scrollback();
                self.last_run = find_last_run(&self.transcript);
                self.pending_plan = None;
                self.plan_flow = PlanFlow::None;
                self.status = self.lang.choose("чат открыт", "chat resumed").to_string();
                self.save_current_config(true);
                self.push_command_result(format!(
                    "{} {}",
                    self.lang.choose("Чат открыт:", "Chat resumed:"),
                    self.chat_id
                ));
            }
            Ok(_) => self.push_command_result(
                self.lang
                    .choose("Чат пустой или повреждён.", "Chat is empty or corrupted."),
            ),
            Err(err) => self.push_command_result(format!(
                "{} {}",
                self.lang
                    .choose("Не удалось открыть чат:", "Failed to open chat:"),
                err
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::chats::testkit::*;

    /// `/clear`: текущий чат уходит с диска целиком, лента заменяется welcome-экраном.
    #[test]
    fn clear_current_chat_removes_the_file_and_starts_a_fresh_context() {
        let (mut app, dir) = app_for_chats();
        let old_path = app.chat_path.clone();
        let keep = write_chat(&dir, "chat-keep", 4);
        app.scrollback_count = 7;

        app.clear_current_chat();

        assert!(!old_path.exists(), "файл текущего чата обязан исчезнуть");
        assert!(keep.exists(), "/clear не трогает другие чаты");
        assert_ne!(app.chat_id, "chat-open");
        assert!(app.chat_path.exists(), "новый чат обязан быть создан");
        assert_eq!(app.transcript, crate::runtime::welcome_lines(&app));
        assert!(!app.transcript.is_empty());
        assert_eq!(app.status, "контекст очищен");
        assert_eq!(app.scrollback_count, 0);
        assert!(app.pending_clear_screen);

        let _ = fs::remove_dir_all(&dir);
    }

    /// `/new`: старый чат остаётся на диске, а работа продолжается в новом.
    #[test]
    fn start_new_chat_switches_to_a_fresh_chat_and_keeps_the_old_one() {
        let (mut app, dir) = app_for_chats();
        let old_path = app.chat_path.clone();

        app.start_new_chat();

        assert!(old_path.exists(), "/new не удаляет прошлый чат");
        assert_ne!(app.chat_id, "chat-open");
        assert!(app.chat_path.exists(), "файл нового чата обязан появиться");
        assert!(!app.chat_title_custom);
        assert!(app.pending_clear_screen);
        assert_eq!(last_line(&app), format!("  ⎿  Новый чат: {}", app.chat_id));

        let _ = fs::remove_dir_all(&dir);
    }

    /// `/resume`: непустой чат открывается, пустой — отвергается (иначе лента обнулится).
    #[test]
    fn resume_chat_opens_only_non_empty_chats() {
        let (mut app, dir) = app_for_chats();
        write_chat(&dir, "chat-empty", 0);
        write_chat(&dir, "chat-full", 2);

        app.resume_chat("chat-empty");
        assert_eq!(app.chat_id, "chat-open", "пустой чат открывать нельзя");
        assert_eq!(last_line(&app), "  ⎿  Чат пустой или повреждён.");

        app.resume_chat("chat-full");
        assert_eq!(app.chat_id, "chat-full");
        assert_eq!(app.transcript[0], "строка 0");
        assert_eq!(app.status, "чат открыт");
        assert_eq!(last_line(&app), "  ⎿  Чат открыт: chat-full");

        app.resume_chat("chat-missing");
        assert_eq!(last_line(&app), "  ⎿  Чат не найден.");

        let _ = fs::remove_dir_all(&dir);
    }
}
