use super::*;

pub(crate) fn handle_onboarding_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        app.handle_ctrl_c();
        return;
    }

    let Some(step) = app.onboarding.as_ref().map(|onboarding| onboarding.step) else {
        return;
    };

    match step {
        OnboardingStep::Provider => handle_onboarding_provider_key(app, key),
        OnboardingStep::Auth => handle_onboarding_auth_key(app, key),
        OnboardingStep::Settings => handle_onboarding_settings_key(app, key),
    }
}

pub(crate) fn handle_onboarding_provider_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => {
            let index = {
                let onboarding = app.onboarding.as_mut().expect("onboarding exists");
                onboarding.provider_index = onboarding.provider_index.saturating_sub(1);
                onboarding.provider_index
            };
            app.set_mode(provider_mode(index));
        }
        KeyCode::Down => {
            let index = {
                let onboarding = app.onboarding.as_mut().expect("onboarding exists");
                onboarding.provider_index =
                    (onboarding.provider_index + 1).min(provider_count() - 1);
                onboarding.provider_index
            };
            app.set_mode(provider_mode(index));
        }
        KeyCode::Enter => {
            let provider_index = app
                .onboarding
                .as_ref()
                .map(|onboarding| onboarding.provider_index);
            if let Some(provider_index) = provider_index {
                app.set_mode(provider_mode(provider_index));
            }
            if let Some(onboarding) = app.onboarding.as_mut() {
                onboarding.step = OnboardingStep::Auth;
                onboarding.message = app
                    .lang
                    .choose(
                        "Проверь авторизацию CLI. Можно запустить логин прямо отсюда.",
                        "Check CLI authentication. You can run login from here.",
                    )
                    .to_string();
            }
        }
        _ => {}
    }
}

pub(crate) fn handle_onboarding_auth_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('c') | KeyCode::Char('C') => {
            app.pending_external = Some(ExternalCommand {
                program: "codex",
                args: &["login"],
                label_ru: "Codex login",
                label_en: "Codex login",
            });
        }
        KeyCode::Char('l') | KeyCode::Char('L') => {
            app.pending_external = Some(ExternalCommand {
                program: "claude",
                args: &["auth", "login"],
                label_ru: "Claude auth login",
                label_en: "Claude auth login",
            });
        }
        KeyCode::Enter => {
            if let Some(onboarding) = app.onboarding.as_mut() {
                onboarding.step = OnboardingStep::Settings;
                onboarding.message = app
                    .lang
                    .choose(
                        "Выставь стартовые настройки. Enter сохранит конфиг.",
                        "Choose startup defaults. Enter saves the config.",
                    )
                    .to_string();
            }
        }
        KeyCode::Backspace | KeyCode::Esc => {
            if let Some(onboarding) = app.onboarding.as_mut() {
                onboarding.step = OnboardingStep::Provider;
            }
        }
        _ => {}
    }
}

pub(crate) fn handle_onboarding_settings_key(app: &mut App, key: KeyEvent) {
    let setting_index = app
        .onboarding
        .as_ref()
        .map(|onboarding| onboarding.setting_index)
        .unwrap_or(0);

    match key.code {
        KeyCode::Up => {
            if let Some(onboarding) = app.onboarding.as_mut() {
                onboarding.setting_index = onboarding.setting_index.saturating_sub(1);
            }
        }
        KeyCode::Down => {
            if let Some(onboarding) = app.onboarding.as_mut() {
                onboarding.setting_index = (onboarding.setting_index + 1).min(2);
            }
        }
        KeyCode::Left => adjust_onboarding_setting(app, setting_index, -1),
        KeyCode::Right => adjust_onboarding_setting(app, setting_index, 1),
        KeyCode::Char('l') | KeyCode::Char('L') => {
            app.lang = if app.lang == Language::Ru {
                Language::En
            } else {
                Language::Ru
            };
        }
        KeyCode::Enter => {
            app.onboarding = None;
            app.status = app.lang.choose("готов", "ready").to_string();
            app.save_current_config(true);
        }
        KeyCode::Backspace | KeyCode::Esc => {
            if let Some(onboarding) = app.onboarding.as_mut() {
                onboarding.step = OnboardingStep::Auth;
            }
        }
        _ => {}
    }
}

pub(crate) fn adjust_onboarding_setting(app: &mut App, setting_index: usize, direction: isize) {
    match setting_index {
        0 => {
            if direction < 0 {
                app.rounds = app.rounds.saturating_sub(1).max(1);
            } else {
                app.rounds = (app.rounds + 1).min(9);
            }
        }
        1 => {
            app.adjust_startup_effort(direction);
        }
        2 => {
            app.lang = if app.lang == Language::Ru {
                Language::En
            } else {
                Language::Ru
            };
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::keytest::*;

    /// `Onboarding::new` зондирует codex/claude настоящими процессами CLI — в юнит-тесте
    /// это флейк, а в CI реальный запуск. Поэтому состояние экрана собираем полями.
    fn onboarding_at(step: OnboardingStep, provider_index: usize) -> Onboarding {
        Onboarding {
            step,
            provider_index,
            setting_index: 0,
            codex_installed: true,
            claude_installed: true,
            codex_authenticated: true,
            claude_authenticated: true,
            codex_status: String::new(),
            claude_status: String::new(),
            message: String::new(),
        }
    }

    fn app_with_onboarding(step: OnboardingStep, provider_index: usize) -> App {
        let mut app = app_for_keys();
        app.set_mode(provider_mode(provider_index));
        app.rounds = 5;
        app.onboarding = Some(onboarding_at(step, provider_index));
        app
    }

    fn onboarding_of(app: &App) -> &Onboarding {
        app.onboarding.as_ref().expect("онбординг открыт")
    }

    #[test]
    fn onboarding_settings_down_stops_at_last_row() {
        let mut app = app_with_onboarding(OnboardingStep::Settings, 0);
        handle_onboarding_settings_key(&mut app, key(KeyCode::Down));
        assert_eq!(onboarding_of(&app).setting_index, 1);
        handle_onboarding_settings_key(&mut app, key(KeyCode::Down));
        assert_eq!(onboarding_of(&app).setting_index, 2);
        handle_onboarding_settings_key(&mut app, key(KeyCode::Down));
        assert_eq!(
            onboarding_of(&app).setting_index,
            2,
            "ниже третьей строки не уходит"
        );
    }

    #[test]
    fn onboarding_settings_up_moves_back() {
        let mut app = app_with_onboarding(OnboardingStep::Settings, 0);
        app.onboarding.as_mut().expect("онбординг").setting_index = 2;
        handle_onboarding_settings_key(&mut app, key(KeyCode::Up));
        assert_eq!(onboarding_of(&app).setting_index, 1);
    }

    #[test]
    fn onboarding_settings_left_right_change_rounds() {
        let mut less = app_with_onboarding(OnboardingStep::Settings, 0);
        handle_onboarding_settings_key(&mut less, key(KeyCode::Left));
        assert_eq!(less.rounds, 4, "← уменьшает раунды");

        let mut more = app_with_onboarding(OnboardingStep::Settings, 0);
        handle_onboarding_settings_key(&mut more, key(KeyCode::Right));
        assert_eq!(more.rounds, 6, "→ увеличивает раунды");
    }

    #[test]
    fn onboarding_settings_l_toggles_language() {
        let mut app = app_with_onboarding(OnboardingStep::Settings, 0);
        handle_onboarding_settings_key(&mut app, key(KeyCode::Char('l')));
        assert_eq!(app.lang, Language::En);

        let mut upper = app_with_onboarding(OnboardingStep::Settings, 0);
        upper.lang = Language::En;
        handle_onboarding_settings_key(&mut upper, key(KeyCode::Char('L')));
        assert_eq!(
            upper.lang,
            Language::Ru,
            "переключатель работает в обе стороны"
        );
    }

    #[test]
    fn onboarding_settings_enter_finishes_onboarding() {
        let mut app = app_with_onboarding(OnboardingStep::Settings, 0);
        handle_onboarding_settings_key(&mut app, key(KeyCode::Enter));
        assert!(app.onboarding.is_none(), "Enter закрывает онбординг");
        assert_eq!(app.status, "готов");
        assert!(app.config_path.exists(), "Enter сохраняет конфиг");
    }

    #[test]
    fn onboarding_settings_backspace_and_esc_return_to_auth() {
        for code in [KeyCode::Backspace, KeyCode::Esc] {
            let mut app = app_with_onboarding(OnboardingStep::Settings, 0);
            handle_onboarding_settings_key(&mut app, key(code));
            assert_eq!(
                onboarding_of(&app).step,
                OnboardingStep::Auth,
                "{code:?} возвращает на шаг авторизации"
            );
        }
    }

    // ───────────────────────── handle_ask_key ─────────────────────────

    #[test]
    fn adjust_onboarding_rounds_by_direction() {
        let mut back = app_with_onboarding(OnboardingStep::Settings, 0);
        adjust_onboarding_setting(&mut back, 0, -1);
        assert_eq!(back.rounds, 4, "отрицательное направление уменьшает");

        let mut forward = app_with_onboarding(OnboardingStep::Settings, 0);
        adjust_onboarding_setting(&mut forward, 0, 1);
        assert_eq!(forward.rounds, 6, "положительное направление увеличивает");

        // Нулевое направление — «вперёд»: единственный вход, различающий `<` и `<=`.
        let mut zero = app_with_onboarding(OnboardingStep::Settings, 0);
        adjust_onboarding_setting(&mut zero, 0, 0);
        assert_eq!(zero.rounds, 6, "направление 0 считается движением вперёд");
    }

    #[test]
    fn adjust_onboarding_rounds_are_clamped() {
        let mut low = app_with_onboarding(OnboardingStep::Settings, 0);
        low.rounds = 1;
        adjust_onboarding_setting(&mut low, 0, -1);
        assert_eq!(low.rounds, 1, "меньше одного раунда не бывает");

        let mut high = app_with_onboarding(OnboardingStep::Settings, 0);
        high.rounds = 9;
        adjust_onboarding_setting(&mut high, 0, 1);
        assert_eq!(high.rounds, 9, "больше девяти раундов не бывает");
    }

    #[test]
    fn adjust_onboarding_startup_effort() {
        let mut less = app_with_onboarding(OnboardingStep::Settings, 3); // ClaudeOnly
        less.claude_effort_index = effort_index_for("high");
        adjust_onboarding_setting(&mut less, 1, -1);
        assert_eq!(effort_label(less.claude_effort_index), "medium");

        let mut more = app_with_onboarding(OnboardingStep::Settings, 3);
        more.claude_effort_index = effort_index_for("high");
        adjust_onboarding_setting(&mut more, 1, 1);
        assert_eq!(effort_label(more.claude_effort_index), "max");
    }

    #[test]
    fn adjust_onboarding_language_toggles() {
        let mut app = app_with_onboarding(OnboardingStep::Settings, 0);
        adjust_onboarding_setting(&mut app, 2, 1);
        assert_eq!(app.lang, Language::En);
        adjust_onboarding_setting(&mut app, 2, 1);
        assert_eq!(
            app.lang,
            Language::Ru,
            "переключатель работает в обе стороны"
        );
    }

    // ───────────────────────── handle_search_key ─────────────────────────

    #[test]
    fn onboarding_provider_down_selects_next_and_stops_at_last() {
        let mut app = app_with_onboarding(OnboardingStep::Provider, 0);
        handle_onboarding_provider_key(&mut app, key(KeyCode::Down));
        assert_eq!(onboarding_of(&app).provider_index, 1);
        assert_eq!(app.mode, Mode::ClaudeCodex, "режим следует за выбором");

        let mut last = app_with_onboarding(OnboardingStep::Provider, 3);
        handle_onboarding_provider_key(&mut last, key(KeyCode::Down));
        assert_eq!(
            onboarding_of(&last).provider_index,
            3,
            "ниже последнего провайдера не уходит"
        );
        assert_eq!(last.mode, Mode::ClaudeOnly);
    }

    #[test]
    fn onboarding_provider_up_selects_previous() {
        let mut app = app_with_onboarding(OnboardingStep::Provider, 2);
        handle_onboarding_provider_key(&mut app, key(KeyCode::Up));
        assert_eq!(onboarding_of(&app).provider_index, 1);
        assert_eq!(app.mode, Mode::ClaudeCodex);
    }

    #[test]
    fn onboarding_provider_enter_goes_to_auth() {
        let mut app = app_with_onboarding(OnboardingStep::Provider, 3);
        handle_onboarding_provider_key(&mut app, key(KeyCode::Enter));
        assert_eq!(
            app.mode,
            Mode::ClaudeOnly,
            "Enter фиксирует выбранный режим"
        );
        let onboarding = onboarding_of(&app);
        assert_eq!(onboarding.step, OnboardingStep::Auth);
        assert!(
            onboarding.message.contains("авторизацию"),
            "подсказка шага авторизации: {}",
            onboarding.message
        );
    }

    // ─────────────────── handle_onboarding_auth_key ───────────────────

    #[test]
    fn onboarding_auth_c_prepares_codex_login() {
        // Команда только кладётся в поле; запускает её позже сам runtime — тест ничего не спавнит.
        for code in [KeyCode::Char('c'), KeyCode::Char('C')] {
            let mut app = app_with_onboarding(OnboardingStep::Auth, 0);
            handle_onboarding_auth_key(&mut app, key(code));
            let command = app.pending_external.as_ref().expect("команда логина");
            assert_eq!(command.program, "codex");
            assert_eq!(command.args, &["login"]);
        }
    }

    #[test]
    fn onboarding_auth_l_prepares_claude_login() {
        for code in [KeyCode::Char('l'), KeyCode::Char('L')] {
            let mut app = app_with_onboarding(OnboardingStep::Auth, 0);
            handle_onboarding_auth_key(&mut app, key(code));
            let command = app.pending_external.as_ref().expect("команда логина");
            assert_eq!(command.program, "claude");
            assert_eq!(command.args, &["auth", "login"]);
        }
    }

    #[test]
    fn onboarding_auth_enter_goes_to_settings() {
        let mut app = app_with_onboarding(OnboardingStep::Auth, 0);
        handle_onboarding_auth_key(&mut app, key(KeyCode::Enter));
        let onboarding = onboarding_of(&app);
        assert_eq!(onboarding.step, OnboardingStep::Settings);
        assert!(
            !onboarding.message.is_empty(),
            "шаг настроек объясняет себя"
        );
    }

    #[test]
    fn onboarding_auth_backspace_and_esc_return_to_provider() {
        for code in [KeyCode::Backspace, KeyCode::Esc] {
            let mut app = app_with_onboarding(OnboardingStep::Auth, 0);
            handle_onboarding_auth_key(&mut app, key(code));
            assert_eq!(
                onboarding_of(&app).step,
                OnboardingStep::Provider,
                "{code:?} возвращает к выбору провайдера"
            );
        }
    }

    // ───────────────────────── handle_shortcuts_key ─────────────────────────

    #[test]
    fn onboarding_dispatches_plain_key_to_current_step() {
        let mut app = app_with_onboarding(OnboardingStep::Auth, 0);
        handle_onboarding_key(&mut app, key(KeyCode::Char('c')));
        let command = app
            .pending_external
            .as_ref()
            .expect("клавиша дошла до шага авторизации");
        assert_eq!(command.program, "codex");
        assert!(app.last_ctrl_c_at.is_none(), "простая «c» — не Ctrl+C");
    }

    #[test]
    fn onboarding_quits_on_double_ctrl_c() {
        let mut app = app_with_onboarding(OnboardingStep::Provider, 1);
        handle_onboarding_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(!app.should_quit);
        handle_onboarding_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(app.should_quit, "двойной Ctrl+C выходит");
        assert_eq!(
            onboarding_of(&app).provider_index,
            1,
            "Ctrl+C до навигации по шагу не доходит"
        );
    }

    // ───────────────────────── handle_marketplace_key ─────────────────────────
}
