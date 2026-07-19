use super::*;

impl App {
    pub(crate) fn open_chats_picker(&mut self) {
        let chats = list_saved_chats(&self.chats_dir, 20);
        if chats.is_empty() {
            self.push_command_result(
                self.lang
                    .choose("Сохранённых чатов пока нет.", "No saved chats yet."),
            );
            return;
        }
        self.chats_index = chats
            .iter()
            .position(|chat| chat.id == self.chat_id)
            .unwrap_or(0);
        self.chats_picker = chats;
        self.overlay = Overlay::Chats;
        self.status = self.lang.choose("чаты", "chats").to_string();
    }

    pub(crate) fn clear_small_chats(&mut self) {
        let chats = list_saved_chats(&self.chats_dir, usize::MAX);
        let mut removed = 0;
        for chat in chats {
            if chat.id == self.chat_id || chat.lines >= 3 {
                continue;
            }
            if let Some(path) = existing_chat_path(&self.chats_dir, &chat.id) {
                if fs::remove_file(&path).is_ok() {
                    removed += 1;
                }
            }
        }
        self.push_command_result(format!(
            "{} {}",
            self.lang
                .choose("Удалено мелких чатов:", "Removed small chats:"),
            removed
        ));
    }

    pub(crate) fn clear_all_chats(&mut self) {
        let chats = list_saved_chats(&self.chats_dir, usize::MAX);
        let mut removed = 0;
        for chat in chats {
            if chat.id == self.chat_id {
                continue;
            }
            if let Some(path) = existing_chat_path(&self.chats_dir, &chat.id) {
                if fs::remove_file(&path).is_ok() {
                    removed += 1;
                }
            }
        }
        self.push_command_result(format!(
            "{} {}",
            self.lang.choose("Удалено чатов:", "Removed chats:"),
            removed
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::chats::testkit::*;

    /// `/clear history` обязан стереть СТАРУЮ историю и не тронуть чат, в котором
    /// пользователь работает прямо сейчас. Инверсия условия выворачивает это наизнанку:
    /// удаляется ровно открытый чат — тихая потеря данных с бодрым «Удалено чатов: 1».
    #[test]
    fn clear_all_chats_wipes_history_and_spares_the_open_chat() {
        let (mut app, dir) = app_for_chats();
        let old_a = write_chat(&dir, "chat-old-a", 3);
        let old_b = write_chat(&dir, "chat-old-b", 3);
        let old_c = write_chat(&dir, "chat-old-c", 3);

        app.clear_all_chats();

        assert!(!old_a.exists(), "старый чат a обязан быть удалён");
        assert!(!old_b.exists(), "старый чат b обязан быть удалён");
        assert!(!old_c.exists(), "старый чат c обязан быть удалён");
        assert!(
            app.chat_path.exists(),
            "текущий чат удалять нельзя: это потеря активных данных"
        );
        assert_eq!(last_line(&app), "  ⎿  Удалено чатов: 3");

        let _ = fs::remove_dir_all(&dir);
    }

    /// `/chats clear` сносит только мелочь (< 3 строк) и никогда — текущий чат.
    #[test]
    fn clear_small_chats_removes_only_short_foreign_chats() {
        let (mut app, dir) = app_for_chats();
        let small = write_chat(&dir, "chat-small", 1);
        let big = write_chat(&dir, "chat-big", 5);

        app.clear_small_chats();

        assert!(!small.exists(), "мелкий чат обязан быть удалён");
        assert!(big.exists(), "чат на 5 строк удалять нельзя");
        assert!(app.chat_path.exists(), "текущий чат удалять нельзя");
        assert_eq!(last_line(&app), "  ⎿  Удалено мелких чатов: 1");

        let _ = fs::remove_dir_all(&dir);
    }

    /// Пикер чатов: без чатов — сообщение и никакого оверлея; с чатами — курсор стоит
    /// на ТЕКУЩЕМ чате, а не на первом попавшемся.
    #[test]
    fn open_chats_picker_selects_the_current_chat() {
        let (mut app, dir) = app_for_chats();
        let _ = fs::remove_file(&app.chat_path);

        app.open_chats_picker();
        assert!(matches!(app.overlay, Overlay::None));
        assert_eq!(last_line(&app), "  ⎿  Сохранённых чатов пока нет.");

        save_chat_transcript(&app.chat_path, &app.chat_id, &app.transcript).expect("save current");
        write_chat(&dir, "chat-a", 2);
        write_chat(&dir, "chat-b", 2);

        app.open_chats_picker();
        assert!(matches!(app.overlay, Overlay::Chats));
        assert_eq!(app.chats_picker.len(), 3);
        assert_eq!(
            app.chats_picker[app.chats_index].id, app.chat_id,
            "курсор обязан стоять на текущем чате"
        );
        assert_eq!(app.status, "чаты");

        let _ = fs::remove_dir_all(&dir);
    }
}
