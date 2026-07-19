use super::*;

mod actions;
mod dispatch;
mod info;
mod presets;

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;

    pub(crate) fn stub_git_ref(_dir: &std::path::Path) -> Option<String> {
        Some("stub".to_string())
    }

    /// Каталог уникален на процесс И на вызов: иначе параллельные прогоны затирают
    /// файлы друг друга и тест начинает падать вразброс.
    pub(crate) fn temp_commands_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = env::temp_dir().join(format!(
            "clave-cmd-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    /// App на временных путях. `App::new()` брать нельзя: она читает настоящий конфиг
    /// и при непройденном онбординге поднимает auth-probe процессы провайдеров.
    ///
    /// `running = true` — намеренно: запускающие команды (`/plan`, `/dev`, `/advisor`, …)
    /// на busy-преflight отвечают сообщением и НЕ поднимают CLI провайдера.
    pub(crate) fn app_for_commands() -> (App, PathBuf) {
        let dir = temp_commands_dir();
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
        app.lang = Language::Ru;
        app.onboarding = None;
        app.overlay = Overlay::None;
        app.git_ref_detector = stub_git_ref;
        app.work_dir = dir.to_string_lossy().to_string();
        app.running = true;
        (app, dir)
    }

    /// Строки, которые команда добавила в ленту (эхо + результат). Команды вроде `/new`
    /// и `/resume` ленту сбрасывают — тогда новой считается вся лента целиком.
    pub(crate) fn run(app: &mut App, line: &str) -> Vec<String> {
        let before = app.transcript.len();
        app.handle_command(line);
        let from = if app.transcript.len() < before {
            0
        } else {
            before
        };
        app.transcript[from..].to_vec()
    }

    pub(crate) fn joined(app: &mut App, line: &str) -> String {
        run(app, line).join("\n")
    }
}
