use super::*;

impl App {
    pub(crate) fn retry_last(&mut self) {
        match self.last_chat_message.clone() {
            Some(message) => self.start_chat(message),
            None => self.push_system(self.lang.choose(
                "Нет последнего запроса для повтора.",
                "No previous request to retry.",
            )),
        }
    }

    pub(crate) fn branch_current_chat(&mut self) {
        let source_id = self.chat_id.clone();
        let transcript = self.transcript.clone();
        self.chat_id = new_chat_id();
        self.chat_path = chat_path_for_id(&self.chats_dir, &self.chat_id);
        self.transcript = transcript;
        self.chat_title_custom = false;
        self.chat_title = chat_display_title(&self.chat_path, &self.transcript, &self.chat_id);
        self.last_run = find_last_run(&self.transcript);

        match save_chat_transcript(&self.chat_path, &self.chat_id, &self.transcript) {
            Ok(()) => {
                self.status = self
                    .lang
                    .choose("ветка создана", "branch created")
                    .to_string();
                self.save_current_config(true);
                self.push_command_result(format!(
                    "{} {} → {}",
                    self.lang
                        .choose("Создана ветка чата:", "Chat branch created:"),
                    source_id,
                    self.chat_id
                ));
            }
            Err(err) => self.push_command_result(format!(
                "{} {}",
                self.lang
                    .choose("Не удалось создать ветку:", "Failed to create branch:"),
                err
            )),
        }
    }

    pub(crate) fn set_work_dir_command(&mut self, rest: &str) {
        let value = rest.trim();
        if value.is_empty() {
            self.push_command_result(self.lang.choose(
                "Использование: /add-dir <папка>",
                "Usage: /add-dir <directory>",
            ));
            return;
        }

        let candidate = PathBuf::from(value);
        let base_dir = launch_work_dir();
        let resolved = if candidate.is_absolute() {
            candidate
        } else {
            base_dir.join(candidate)
        };

        if !resolved.is_dir() {
            self.push_command_result(format!(
                "{} {}",
                self.lang
                    .choose("Папка не найдена:", "Directory does not exist:"),
                resolved.display()
            ));
            return;
        }

        self.work_dir = resolved.to_string_lossy().to_string();
        // Индикатор футера — про НОВЫЙ каталог, и уже в этом кадре.
        self.refresh_git_ref();
        self.status = self.lang.choose("cwd обновлён", "cwd updated").to_string();
        self.save_current_config(true);
        self.push_command_result(format!(
            "{} {}",
            self.lang
                .choose("Рабочая директория:", "Working directory:"),
            self.work_dir
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::testkit::*;

    /// Смена рабочего каталога обязана обновить индикатор футера в ЭТОМ же кадре: иначе
    /// футер продолжал бы показывать ветку прошлого каталога.
    #[test]
    fn set_work_dir_moves_the_cwd_and_refreshes_the_git_indicator() {
        let (mut app, dir) = app_for_commands();
        app.git_ref = None;

        app.set_work_dir_command(&dir.to_string_lossy());
        assert_eq!(app.work_dir, dir.to_string_lossy());
        assert_eq!(app.git_ref.as_deref(), Some("stub"));

        // Несуществующая папка: ни каталог, ни индикатор не трогаем.
        app.git_ref = None;
        app.set_work_dir_command(&dir.join("missing").to_string_lossy());
        assert_eq!(app.work_dir, dir.to_string_lossy());
        assert_eq!(app.git_ref, None);

        let _ = fs::remove_dir_all(&dir);
    }
}
