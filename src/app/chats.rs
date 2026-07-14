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

impl App {
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

    /// Каталог уникален на процесс И на вызов: параллельные прогоны иначе затирают
    /// файлы друг друга, и мутационный гейт получает случайные падения.
    fn temp_chats_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = std::env::temp_dir().join(format!(
            "clave-chats-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    /// App на своих временных путях. Через `App::new()` нельзя: она читает настоящий
    /// конфиг пользователя и при непройденном онбординге поднимает auth-probe процессы.
    fn app_for_chats() -> (App, PathBuf) {
        let dir = temp_chats_dir();
        let config = AppConfig {
            onboarding_done: true,
            ..AppConfig::default()
        };
        let mut app = App::from_config(
            config,
            dir.join("config.json"),
            dir.join("history"),
            dir.clone(),
        );
        // from_config уже создала свой стартовый чат — убираем его и берём собственный
        // текущий чат с известным id и известным числом строк.
        let _ = fs::remove_file(&app.chat_path);
        app.lang = Language::Ru;
        app.onboarding = None;
        app.overlay = Overlay::None;
        app.chat_id = "chat-open".to_string();
        app.chat_path = chat_path_for_id(&dir, "chat-open");
        app.transcript = vec!["◆ привет".to_string()];
        save_chat_transcript(&app.chat_path, &app.chat_id, &app.transcript).expect("save current");
        (app, dir)
    }

    fn write_chat(dir: &Path, id: &str, lines: usize) -> PathBuf {
        let path = chat_path_for_id(dir, id);
        let transcript = (0..lines)
            .map(|i| format!("строка {i}"))
            .collect::<Vec<_>>();
        save_chat_transcript(&path, id, &transcript).expect("save chat");
        path
    }

    fn last_line(app: &App) -> String {
        app.transcript.last().cloned().unwrap_or_default()
    }

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
