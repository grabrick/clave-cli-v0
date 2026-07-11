use super::*;

pub(crate) fn draw_settings_screen(frame: &mut Frame<'_>, area: Rect, app: &App) {
    frame.render_widget(Clear, area);

    let mut lines = vec![Line::styled(
        "› /settings",
        Style::default()
            .fg(app.theme.accent())
            .add_modifier(Modifier::BOLD),
    )];
    lines.push(Line::styled(
        "─".repeat(area.width as usize),
        Style::default().fg(app.theme.accent_dim()),
    ));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        app.lang.choose("Настройки", "Settings"),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    push_settings_row(
        &mut lines,
        app,
        0,
        app.lang.choose("Простой чат", "Direct chat"),
        app.direct_provider.title(),
        app.lang.choose(
            "кто отвечает на обычный Enter без /plan",
            "model for plain Enter messages without /plan",
        ),
    );
    push_settings_row(
        &mut lines,
        app,
        1,
        app.lang.choose("Исполнитель", "Executor"),
        app.mode.architect_provider().title(),
        app.lang.choose(
            "кто пишет план или первичный вариант решения",
            "who drafts the plan or first solution",
        ),
    );
    push_settings_row(
        &mut lines,
        app,
        2,
        app.lang.choose("Ревьюер", "Reviewer"),
        app.mode.reviewer_provider().title(),
        app.lang.choose(
            "кто проверяет, спорит и режет лишний scope",
            "who reviews, challenges, and trims scope",
        ),
    );
    push_settings_row(
        &mut lines,
        app,
        3,
        app.lang.choose("Цвет", "Theme"),
        app.theme.title(),
        app.lang
            .choose("цветовая гамма интерфейса", "terminal color palette"),
    );
    push_settings_row(
        &mut lines,
        app,
        4,
        app.lang.choose("Раунды", "Rounds"),
        &app.rounds.to_string(),
        app.lang.choose("лимит раундов Clave", "Clave round limit"),
    );
    push_settings_row(
        &mut lines,
        app,
        5,
        app.lang.choose("Язык", "Language"),
        app.lang.as_str(),
        app.lang
            .choose("язык интерфейса Clave", "Clave interface language"),
    );
    push_settings_row(
        &mut lines,
        app,
        6,
        app.lang.choose("Открытие путей", "Open paths"),
        app.path_link_target.label(app.lang),
        app.lang.choose(
            "чем открывать пути по Cmd+клику",
            "what opens file paths on Cmd+click",
        ),
    );

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        app.lang.choose(
            "↑/↓ выбрать · ←/→ изменить · Enter сохранить · Esc отменить",
            "↑/↓ select · ←/→ change · Enter save · Esc cancel",
        ),
        Style::default().fg(MUTED),
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn push_settings_row(
    lines: &mut Vec<Line<'static>>,
    app: &App,
    index: usize,
    label: &'static str,
    value: &str,
    hint: &'static str,
) {
    let focused = app.settings_focus == index;
    let prefix = if focused { "› " } else { "  " };
    let mut label_style = Style::default().fg(Color::White);
    let mut value_style = Style::default().fg(app.theme.accent_soft());
    if focused {
        label_style = label_style.add_modifier(Modifier::BOLD);
        value_style = value_style
            .fg(Color::White)
            .bg(app.theme.accent_bg())
            .add_modifier(Modifier::BOLD);
    }

    lines.push(Line::from(vec![
        Span::styled(prefix, Style::default().fg(app.theme.accent())),
        Span::styled(format!("{label:<14} "), label_style),
        Span::styled(format!(" {value} "), value_style),
        Span::raw("  "),
        Span::styled(hint.to_string(), Style::default().fg(MUTED)),
    ]));
}
