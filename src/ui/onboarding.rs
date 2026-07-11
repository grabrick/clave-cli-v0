use super::*;

pub(crate) fn draw_onboarding(frame: &mut Frame<'_>, area: Rect, app: &App) {
    frame.render_widget(Clear, area);
    let Some(onboarding) = app.onboarding.as_ref() else {
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(1)])
        .split(area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(vec![
            Span::styled(
                format!(" {APP_NAME} Setup "),
                Style::default()
                    .fg(app.theme.accent())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("first run ", Style::default().fg(MUTED)),
        ]))
        .border_style(Style::default().fg(app.theme.accent()));
    frame.render_widget(block, chunks[0]);

    let inner = Rect {
        x: chunks[0].x + 2,
        y: chunks[0].y + 1,
        width: chunks[0].width.saturating_sub(4),
        height: chunks[0].height.saturating_sub(2),
    };

    let lines = match onboarding.step {
        OnboardingStep::Provider => onboarding_provider_lines(app, onboarding),
        OnboardingStep::Auth => onboarding_auth_lines(app, onboarding),
        OnboardingStep::Settings => onboarding_settings_lines(app, onboarding),
    };

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    draw_footer(frame, chunks[1], app);
}

pub(crate) fn onboarding_provider_lines(app: &App, onboarding: &Onboarding) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::styled(
            app.lang
                .choose("Выбор связки моделей", "Choose model pairing"),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(onboarding.message.clone()),
        Line::from(""),
    ];

    for index in 0..provider_count() {
        let selected = index == onboarding.provider_index;
        let mode = provider_mode(index);
        let style = if selected {
            Style::default()
                .fg(Color::White)
                .bg(app.theme.accent_bg())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.accent_soft())
        };
        lines.push(Line::from(vec![
            Span::styled(
                if selected { "› " } else { "  " },
                Style::default().fg(app.theme.accent()),
            ),
            Span::styled(format!("{:<14}", mode.as_str()), style),
            Span::raw(" "),
            Span::styled(
                provider_description(mode, app.lang),
                Style::default().fg(MUTED),
            ),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::styled(
        app.lang.choose(
            "↑/↓ выбрать · Enter продолжить · Ctrl+C выйти",
            "↑/↓ choose · Enter continue · Ctrl+C exit",
        ),
        Style::default().fg(MUTED),
    ));
    lines
}

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

pub(crate) fn onboarding_settings_lines(app: &App, onboarding: &Onboarding) -> Vec<Line<'static>> {
    let rows = [
        (
            app.lang.choose("Раунды ревью", "Review rounds").to_string(),
            app.rounds.to_string(),
        ),
        ("Effort".to_string(), app.effort_summary()),
        (
            app.lang.choose("Язык", "Language").to_string(),
            app.lang.as_str().to_string(),
        ),
    ];

    let mut lines = vec![
        Line::styled(
            app.lang.choose("Стартовые настройки", "Startup settings"),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(onboarding.message.clone()),
        Line::from(""),
    ];

    for (index, (label, value)) in rows.into_iter().enumerate() {
        let selected = index == onboarding.setting_index;
        let style = if selected {
            Style::default()
                .fg(Color::White)
                .bg(app.theme.accent_bg())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.accent_soft())
        };
        lines.push(Line::from(vec![
            Span::styled(
                if selected { "› " } else { "  " },
                Style::default().fg(app.theme.accent()),
            ),
            Span::styled(format!("{label:<18}"), style),
            Span::raw(" "),
            Span::styled(value, Style::default().fg(Color::White)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            app.lang.choose("Режим ", "Mode "),
            Style::default().fg(MUTED),
        ),
        Span::styled(
            app.mode.as_str(),
            Style::default()
                .fg(app.theme.accent_soft())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.lang.choose(" · Артефакты ", " · Artifacts "),
            Style::default().fg(MUTED),
        ),
        Span::styled(
            app.out_dir.clone(),
            Style::default().fg(app.theme.accent_soft()),
        ),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::styled(
        app.lang.choose(
            "↑/↓ поле · ←/→ изменить · L язык · Enter сохранить · Esc назад",
            "↑/↓ field · ←/→ change · L language · Enter save · Esc back",
        ),
        Style::default().fg(MUTED),
    ));
    lines
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
