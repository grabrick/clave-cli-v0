use super::*;

#[derive(Clone, Copy)]
pub(crate) struct SettingsSnapshot {
    pub(crate) direct_provider: Provider,
    pub(crate) theme: Theme,
    pub(crate) mode: Mode,
    pub(crate) rounds: usize,
    pub(crate) lang: Language,
    pub(crate) path_link_target: PathTarget,
}

impl App {
    pub(crate) fn settings_snapshot(&self) -> SettingsSnapshot {
        SettingsSnapshot {
            direct_provider: self.direct_provider,
            theme: self.theme,
            mode: self.mode,
            rounds: self.rounds,
            lang: self.lang,
            path_link_target: self.path_link_target,
        }
    }

    pub(crate) fn restore_settings_snapshot(&mut self, snapshot: SettingsSnapshot) {
        self.direct_provider = snapshot.direct_provider;
        self.theme = snapshot.theme;
        self.set_mode(snapshot.mode);
        self.rounds = snapshot.rounds;
        self.lang = snapshot.lang;
        self.path_link_target = snapshot.path_link_target;
    }

    pub(crate) fn open_settings(&mut self) {
        self.open_settings_from();
    }

    pub(crate) fn open_settings_from(&mut self) {
        self.settings_original = Some(self.settings_snapshot());
        self.settings_focus = 0;
        self.overlay = Overlay::Settings;
        self.status = "settings".to_string();
    }

    pub(crate) fn settings_rows(&self) -> usize {
        7
    }

    pub(crate) fn adjust_settings_focus(&mut self, direction: isize) {
        if direction < 0 {
            self.settings_focus = self.settings_focus.saturating_sub(1);
        } else {
            self.settings_focus = (self.settings_focus + 1).min(self.settings_rows() - 1);
        }
    }

    pub(crate) fn adjust_settings_value(&mut self, direction: isize) {
        match self.settings_focus {
            0 => self.direct_provider = self.direct_provider.toggled(),
            1 => {
                let architect = self.mode.architect_provider().toggled();
                let reviewer = self.mode.reviewer_provider();
                self.set_mode(Mode::from_roles(architect, reviewer));
            }
            2 => {
                let architect = self.mode.architect_provider();
                let reviewer = self.mode.reviewer_provider().toggled();
                self.set_mode(Mode::from_roles(architect, reviewer));
            }
            3 => self.theme = self.theme.shifted(direction),
            4 => {
                if direction < 0 {
                    self.rounds = self.rounds.saturating_sub(1).max(1);
                } else {
                    self.rounds = (self.rounds + 1).min(9);
                }
            }
            5 => {
                self.lang = if self.lang == Language::Ru {
                    Language::En
                } else {
                    Language::Ru
                };
            }
            6 => {
                let targets = available_targets(editor_installed);
                let count = targets.len();
                let current = targets
                    .iter()
                    .position(|target| *target == self.path_link_target)
                    .unwrap_or(0);
                let next = if direction < 0 {
                    (current + count - 1) % count
                } else {
                    (current + 1) % count
                };
                self.path_link_target = targets[next];
            }
            _ => {}
        }
    }

    pub(crate) fn set_direct_provider(&mut self, provider: Provider) {
        self.direct_provider = provider;
        self.save_current_config(true);
        self.push_command_result(format!(
            "{} {}.",
            self.lang
                .choose("Модель для простых сообщений:", "Direct chat model set to:"),
            provider.title()
        ));
    }

    pub(crate) fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.save_current_config(true);
        self.push_command_result(format!(
            "{} {}.",
            self.lang.choose("Цветовая гамма:", "Theme set to:"),
            theme.title()
        ));
    }

    pub(crate) fn set_roles(&mut self, architect: Provider, reviewer: Provider) {
        self.set_mode(Mode::from_roles(architect, reviewer));
        self.save_current_config(true);
        self.push_command_result(format!(
            "{} {} → {}.",
            self.lang.choose("Роли планирования:", "Planning roles:"),
            architect.title(),
            reviewer.title()
        ));
        self.ensure_auth_ready_for_current_mode();
    }

    pub(crate) fn settings_summary(&self) -> String {
        format!(
            "chat {} · theme {} · roles {}>{}",
            self.direct_provider.as_str(),
            self.theme.as_str(),
            self.mode.architect_provider().as_str(),
            self.mode.reviewer_provider().as_str()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Каталог уникален на процесс И на вызов: параллельные прогоны иначе затирают
    /// файлы друг друга, и мутационный гейт получает случайные падения.
    fn temp_settings_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = std::env::temp_dir().join(format!(
            "clave-settings-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    /// App на своих временных путях. Через `App::new()` нельзя: она читает настоящий
    /// конфиг пользователя и при непройденном онбординге поднимает auth-probe процессы.
    fn app_for_settings() -> App {
        let dir = temp_settings_dir();
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
        app
    }

    /// Фокус ходит по строкам и упирается в границы — за последнюю строку не уезжает,
    /// выше первой не поднимается. Нулевое направление — шаг вперёд.
    #[test]
    fn settings_focus_moves_within_the_rows() {
        let mut app = app_for_settings();
        app.settings_focus = 0;

        app.adjust_settings_focus(-1);
        assert_eq!(app.settings_focus, 0, "выше первой строки не уходим");

        app.adjust_settings_focus(0);
        assert_eq!(app.settings_focus, 1, "нулевое направление — шаг вперёд");

        for _ in 0..10 {
            app.adjust_settings_focus(1);
        }
        assert_eq!(
            app.settings_focus,
            app.settings_rows() - 1,
            "ниже последней строки не уезжаем"
        );

        app.adjust_settings_focus(-1);
        assert_eq!(app.settings_focus, app.settings_rows() - 2);
    }

    /// Строка 0 — модель для простых сообщений.
    #[test]
    fn row_zero_switches_the_direct_provider() {
        let mut app = app_for_settings();
        app.settings_focus = 0;
        app.direct_provider = Provider::Codex;

        app.adjust_settings_value(1);

        assert_eq!(app.direct_provider, Provider::Claude);
    }

    /// Строки 1 и 2 — архитектор и ревьюер, каждая правит СВОЮ роль и не трогает чужую.
    #[test]
    fn rows_one_and_two_switch_architect_and_reviewer_separately() {
        let mut app = app_for_settings();
        app.set_mode(Mode::CodexOnly);
        app.settings_focus = 1;

        app.adjust_settings_value(1);

        assert_eq!(app.mode.architect_provider(), Provider::Claude);
        assert_eq!(
            app.mode.reviewer_provider(),
            Provider::Codex,
            "ревьюера строка архитектора не трогает"
        );

        let mut app = app_for_settings();
        app.set_mode(Mode::CodexOnly);
        app.settings_focus = 2;

        app.adjust_settings_value(1);

        assert_eq!(app.mode.reviewer_provider(), Provider::Claude);
        assert_eq!(
            app.mode.architect_provider(),
            Provider::Codex,
            "архитектора строка ревьюера не трогает"
        );
    }

    /// Строка 3 — тема: вперёд и назад дают РАЗНЫЕ темы.
    #[test]
    fn row_three_shifts_the_theme_both_ways() {
        let mut app = app_for_settings();
        app.settings_focus = 3;
        app.theme = Theme::ALL[0];

        app.adjust_settings_value(1);
        assert_eq!(app.theme, Theme::ALL[1], "вперёд — следующая тема");

        app.adjust_settings_value(-1);
        assert_eq!(app.theme, Theme::ALL[0], "назад — предыдущая");
    }

    /// Строка 4 — раунды: 1..9, нулевое направление увеличивает.
    #[test]
    fn row_four_clamps_the_rounds() {
        let mut app = app_for_settings();
        app.settings_focus = 4;
        app.rounds = 3;

        app.adjust_settings_value(-1);
        assert_eq!(app.rounds, 2);

        app.adjust_settings_value(0);
        assert_eq!(app.rounds, 3, "нулевое направление — вверх");

        app.rounds = 1;
        app.adjust_settings_value(-1);
        assert_eq!(app.rounds, 1, "ниже одного раунда не опускаемся");

        app.rounds = 9;
        app.adjust_settings_value(1);
        assert_eq!(app.rounds, 9, "выше девяти не поднимаемся");
    }

    /// Строка 5 — язык: переключается в обе стороны.
    #[test]
    fn row_five_toggles_the_language() {
        let mut app = app_for_settings();
        app.settings_focus = 5;
        app.lang = Language::Ru;

        app.adjust_settings_value(1);
        assert_eq!(app.lang, Language::En);

        app.adjust_settings_value(1);
        assert_eq!(app.lang, Language::Ru);
    }

    /// Строка 6 — цель Cmd+клика: список обходится по кругу в обе стороны.
    #[test]
    fn row_six_cycles_the_path_link_target() {
        let targets = available_targets(editor_installed);
        let count = targets.len();

        let mut app = app_for_settings();
        app.settings_focus = 6;
        app.path_link_target = targets[0];

        app.adjust_settings_value(1);
        assert_eq!(app.path_link_target, targets[1], "вперёд — следующая цель");

        app.path_link_target = targets[0];
        app.adjust_settings_value(0);
        assert_eq!(
            app.path_link_target, targets[1],
            "нулевое направление — тоже вперёд"
        );

        app.path_link_target = targets[0];
        app.adjust_settings_value(-1);
        assert_eq!(
            app.path_link_target,
            targets[count - 1],
            "назад с первой цели — на последнюю"
        );
    }

    /// Сводка настроек — точная строка: её читают в футере и в `/settings`.
    #[test]
    fn settings_summary_shows_provider_theme_and_roles() {
        let mut app = app_for_settings();
        app.direct_provider = Provider::Claude;
        app.theme = Theme::Cyan;
        app.set_mode(Mode::ClaudeCodex);

        assert_eq!(
            app.settings_summary(),
            "chat claude · theme cyan · roles claude>codex"
        );
    }
}
