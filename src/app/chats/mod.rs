use super::*;

mod buffer;
mod lifecycle;
mod list;
mod title;

#[cfg(test)]
pub(crate) mod testkit {
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
    pub(crate) fn app_for_chats() -> (App, PathBuf) {
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

    pub(crate) fn write_chat(dir: &Path, id: &str, lines: usize) -> PathBuf {
        let path = chat_path_for_id(dir, id);
        let transcript = (0..lines)
            .map(|i| format!("строка {i}"))
            .collect::<Vec<_>>();
        save_chat_transcript(&path, id, &transcript).expect("save chat");
        path
    }

    pub(crate) fn last_line(app: &App) -> String {
        app.transcript.last().cloned().unwrap_or_default()
    }
}
