use super::*;

mod answer;
mod lifecycle;
mod state;
pub(crate) use answer::*;
pub(crate) use state::*;

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;

    /// Каталог уникален на процесс И на вызов: параллельные прогоны иначе затирают
    /// файлы друг друга, и мутационный гейт получает случайные падения.
    pub(crate) fn temp_ask_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = std::env::temp_dir().join(format!(
            "clave-ask-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    /// App на своих временных путях. `running = true` — обязателен всюду, где ответ уходит
    /// модели: тогда сообщение встаёт в очередь и живой CLI провайдера не поднимается.
    pub(crate) fn app_for_ask() -> App {
        let dir = temp_ask_dir();
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
        app.transcript.clear();
        app
    }

    pub(crate) fn question(
        text: &str,
        multi: bool,
        options: &[&str],
        allow_custom: bool,
    ) -> AskQuestion {
        AskQuestion {
            question: text.to_string(),
            multi,
            options: options
                .iter()
                .map(|label| AskOption {
                    label: (*label).to_string(),
                    note: None,
                })
                .collect(),
            allow_custom,
        }
    }

    pub(crate) fn open_ask(app: &mut App, questions: Vec<AskQuestion>) {
        app.ask = Some(AskState::new(AskPrompt { questions }));
    }

    pub(crate) fn state(app: &mut App) -> &mut AskState {
        app.ask.as_mut().expect("селектор открыт")
    }
}
