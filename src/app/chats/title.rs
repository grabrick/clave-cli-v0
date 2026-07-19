use super::*;

impl App {
    pub(crate) fn refresh_current_chat_title(&mut self) {
        self.chat_title_custom = read_chat_title(&self.chat_path).is_some();
        self.chat_title = chat_display_title(&self.chat_path, &self.transcript, &self.chat_id);
    }

    pub(crate) fn set_chat_title_from_prompt_if_needed(&mut self, prompt: &str) {
        if self.chat_title_custom || first_prompt_title(&self.transcript).is_some() {
            return;
        }

        let title = truncate_chars(prompt.trim(), 72);
        if !title.is_empty() {
            self.chat_title = title;
        }
    }

    pub(crate) fn rename_current_chat(&mut self, title: &str) {
        let title = title.trim();
        if title.is_empty() {
            self.push_command_result(
                self.lang
                    .choose("Использование: /name <заголовок>", "Usage: /name <title>"),
            );
            return;
        }
        match set_chat_title(&self.chat_path, &self.chat_id, title) {
            Ok(()) => {
                self.chat_title = truncate_chars(title, 72);
                self.chat_title_custom = true;
                self.push_command_result(format!(
                    "{} {}",
                    self.lang.choose("Чат назван:", "Chat named:"),
                    title
                ));
            }
            Err(err) => self.push_command_result(format!(
                "{} {}",
                self.lang
                    .choose("Не удалось переименовать:", "Failed to rename:"),
                err
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::chats::testkit::*;

    /// `/name`: имя пишется в файл чата, пустой аргумент даёт подсказку.
    #[test]
    fn rename_current_chat_persists_the_title() {
        let (mut app, dir) = app_for_chats();

        app.rename_current_chat("   ");
        assert_eq!(last_line(&app), "  ⎿  Использование: /name <заголовок>");
        assert!(!app.chat_title_custom);

        app.rename_current_chat("Мой чат");
        assert_eq!(app.chat_title, "Мой чат");
        assert!(app.chat_title_custom);
        assert_eq!(
            read_chat_title(&app.chat_path).as_deref(),
            Some("Мой чат"),
            "имя обязано пережить перезапуск, значит лежать в файле"
        );
        assert_eq!(last_line(&app), "  ⎿  Чат назван: Мой чат");

        let _ = fs::remove_dir_all(&dir);
    }

    /// Заголовок перечитывается из файла: иначе после /resume в шапке висит старое имя.
    #[test]
    fn refresh_current_chat_title_reads_the_title_from_disk() {
        let (mut app, dir) = app_for_chats();
        set_chat_title(&app.chat_path, &app.chat_id, "Имя из файла").expect("set title");
        app.chat_title = "устаревшее".to_string();
        app.chat_title_custom = false;

        app.refresh_current_chat_title();

        assert_eq!(app.chat_title, "Имя из файла");
        assert!(app.chat_title_custom);

        let _ = fs::remove_dir_all(&dir);
    }

    /// Автозаголовок берётся из первого промпта — но только если своего имени ещё нет.
    #[test]
    fn chat_title_from_prompt_respects_custom_and_existing_titles() {
        let (mut app, dir) = app_for_chats();
        app.transcript.clear();
        app.chat_title_custom = false;
        app.chat_title = "chat-open".to_string();

        app.set_chat_title_from_prompt_if_needed("Починить футер");
        assert_eq!(app.chat_title, "Починить футер");

        // Пробельный промпт не имеет права затирать уже вычисленный заголовок.
        app.set_chat_title_from_prompt_if_needed("   ");
        assert_eq!(app.chat_title, "Починить футер");

        // Своё имя (/name) сильнее любого промпта.
        app.chat_title_custom = true;
        app.chat_title = "Кастом".to_string();
        app.set_chat_title_from_prompt_if_needed("Другая задача");
        assert_eq!(app.chat_title, "Кастом");

        let _ = fs::remove_dir_all(&dir);
    }
}
