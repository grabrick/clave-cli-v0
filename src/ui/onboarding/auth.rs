use super::*;

pub(crate) fn onboarding_auth_lines(app: &App, onboarding: &Onboarding) -> Vec<Line<'static>> {
    let codex_needed = app.mode.needs_codex();
    let claude_needed = app.mode.needs_claude();
    vec![
        Line::styled(
            app.lang.choose("Авторизация CLI", "CLI authentication"),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Line::from(onboarding.message.clone()),
        Line::from(""),
        auth_status_line(
            "Codex",
            codex_needed,
            onboarding.codex_installed,
            onboarding.codex_authenticated,
            &onboarding.codex_status,
            "codex login",
            "C",
            app.lang,
            app.theme,
        ),
        auth_status_line(
            "Claude",
            claude_needed,
            onboarding.claude_installed,
            onboarding.claude_authenticated,
            &onboarding.claude_status,
            "claude auth login",
            "L",
            app.lang,
            app.theme,
        ),
        Line::from(""),
        Line::styled(
            app.lang.choose(
                "C запустить Codex login · L запустить Claude auth login · Enter дальше · Esc назад",
                "C run Codex login · L run Claude auth login · Enter next · Esc back",
            ),
            Style::default().fg(MUTED),
        ),
    ]
}

// Связный набор данных строки статуса авторизации; дробить ради порога lint не нужно.
#[allow(clippy::too_many_arguments)]
pub(crate) fn auth_status_line(
    name: &'static str,
    needed: bool,
    installed: bool,
    authenticated: bool,
    status_text: &str,
    command: &'static str,
    key: &'static str,
    lang: Language,
    theme: Theme,
) -> Line<'static> {
    let need_label = if needed {
        lang.choose("нужен", "needed")
    } else {
        lang.choose("опционально", "optional")
    };
    let status = if !installed {
        lang.choose("CLI не найден", "CLI missing").to_string()
    } else if authenticated {
        lang.choose("аккаунт готов", "account ready").to_string()
    } else {
        lang.choose("не авторизован", "not logged in").to_string()
    };
    let status_style = if installed && authenticated {
        Style::default()
            .fg(theme.accent_soft())
            .add_modifier(Modifier::BOLD)
    } else if installed {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    };
    let detail = truncate_chars(status_text, 36);

    Line::from(vec![
        Span::styled(
            format!("{name:<8}"),
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{need_label:<12}"), Style::default().fg(MUTED)),
        Span::styled(status, status_style),
        Span::raw(" · "),
        Span::styled(format!("{key}: {command}"), Style::default().fg(MUTED)),
        Span::raw(" · "),
        Span::styled(detail, Style::default().fg(Color::DarkGray)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::onboarding::testkit::*;

    /// Экран авторизации показывает заголовок и обе строки CLI.
    #[test]
    fn auth_lines_show_the_header_and_both_clis() {
        let app = onboarding_app();
        let onb = onboarding(OnboardingStep::Auth);
        let lines = onboarding_auth_lines(&app, &onb);

        // 95:5 `→ vec![]` и `→ vec![Default::default()]`: осмысленный непустой контент.
        let text = joined(&lines);
        assert!(text.contains("CLI authentication"), "нет заголовка");
        assert!(text.contains("Codex"), "нет строки Codex");
        assert!(text.contains("Claude"), "нет строки Claude");
    }

    // ── Часть 4. onboarding_settings_lines ───────────────────────────────────

    /// Строка статуса собрана полностью: ровно 7 спанов с точным содержимым.
    #[test]
    fn auth_status_line_builds_all_seven_spans() {
        let line = auth_status_line(
            "Codex",
            true,
            true,
            true,
            "",
            "codex login",
            "C",
            Language::En,
            Theme::Purple,
        );

        // 227:5 `→ Default::default()`: пустая Line без спанов.
        assert_eq!(
            span_texts(&line),
            vec![
                "Codex   ".to_string(),
                "needed      ".to_string(),
                "account ready".to_string(),
                " · ".to_string(),
                "C: codex login".to_string(),
                " · ".to_string(),
                String::new(),
            ]
        );
    }

    /// Не установленный CLI показывает «CLI missing», а не статус авторизации.
    #[test]
    fn auth_status_line_reports_a_missing_cli() {
        let line = auth_status_line(
            "Codex",
            true,
            false,
            false,
            "",
            "codex login",
            "C",
            Language::En,
            Theme::Purple,
        );

        // 232:21 `delete !`: с `if installed` вместо `if !installed` ушли бы в
        // else-ветку → «not logged in».
        assert_eq!(line.spans[2].content.as_ref(), "CLI missing");
    }

    /// Установленный, но не авторизованный CLI подсвечивается жёлтым.
    #[test]
    fn auth_status_line_colours_an_unauthenticated_cli_yellow() {
        let line = auth_status_line(
            "Codex",
            true,
            true,
            false,
            "",
            "codex login",
            "C",
            Language::En,
            Theme::Purple,
        );

        // 239:37 `&& → ||`: installed && authenticated = false → ветка `else if
        // installed` → Yellow. При `||` было бы true → accent_soft (Indexed(183)).
        assert_eq!(line.spans[2].style.fg, Some(Color::Yellow));
        assert_ne!(
            Theme::Purple.accent_soft(),
            Color::Yellow,
            "жёлтый обязан отличаться от accent_soft"
        );
    }
}
