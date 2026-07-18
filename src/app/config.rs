use super::*;

#[derive(Clone)]
pub(crate) struct AppConfig {
    pub(crate) onboarding_done: bool,
    pub(crate) mode: Mode,
    /// Режим прямого чата (Shift+Tab): Discussion/Plan/FullAccess/Tandem. Переживает
    /// перезапуск — иначе Tandem каждый раз сбрасывается на Discussion.
    pub(crate) chat_mode: ChatMode,
    pub(crate) direct_provider: Provider,
    pub(crate) theme: Theme,
    pub(crate) lang: Language,
    pub(crate) rounds: usize,
    pub(crate) work_dir: String,
    pub(crate) out_dir: String,
    pub(crate) effort_index: usize,
    pub(crate) codex_effort_index: usize,
    pub(crate) claude_effort_index: usize,
    pub(crate) linked_effort_split: bool,
    pub(crate) last_chat_id: Option<String>,
    /// Цель открытия путей. `None` → не задано в конфиге, App применит авто-детект.
    pub(crate) path_link_target: Option<PathTarget>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            onboarding_done: false,
            mode: Mode::CodexOnly,
            chat_mode: ChatMode::Discussion,
            direct_provider: Provider::Codex,
            theme: Theme::Purple,
            lang: Language::Ru,
            rounds: 2,
            work_dir: ".".to_string(),
            out_dir: DEFAULT_ARTIFACT_DIR.to_string(),
            effort_index: 3,
            codex_effort_index: 3,
            claude_effort_index: 4,
            linked_effort_split: true,
            last_chat_id: None,
            path_link_target: None,
        }
    }
}

impl App {
    pub(crate) fn current_config(&self, onboarding_done: bool) -> AppConfig {
        AppConfig {
            onboarding_done,
            mode: self.mode,
            chat_mode: self.chat_mode,
            direct_provider: self.direct_provider,
            theme: self.theme,
            lang: self.lang,
            rounds: self.rounds,
            work_dir: self.work_dir.clone(),
            out_dir: self.out_dir.clone(),
            effort_index: self.effort_index,
            codex_effort_index: self.codex_effort_index,
            claude_effort_index: self.claude_effort_index,
            linked_effort_split: self.linked_effort_split,
            last_chat_id: Some(self.chat_id.clone()),
            path_link_target: Some(self.path_link_target),
        }
    }

    pub(crate) fn save_current_config(&mut self, onboarding_done: bool) {
        if let Err(err) = save_config(&self.config_path, &self.current_config(onboarding_done)) {
            self.push_system(format!(
                "{} {}",
                self.lang
                    .choose("Не удалось сохранить конфиг:", "Failed to save config:"),
                err
            ));
        }
    }
}
