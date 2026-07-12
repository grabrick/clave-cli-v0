use super::*;

pub(crate) fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.height == 0 {
        return;
    }

    if let Some((message, shown_at)) = &app.footer_notice {
        if shown_at.elapsed() <= Duration::from_secs(2) {
            let text = truncate_chars(message, area.width as usize);
            frame.render_widget(
                Paragraph::new(text).style(
                    Style::default()
                        .fg(app.theme.accent_soft())
                        .add_modifier(Modifier::BOLD),
                ),
                area,
            );
            return;
        }
    }

    let mode_label = app.chat_mode.label(app.lang);
    let switch = MODE_SWITCH_KEYS;
    let hints = app
        .lang
        .choose("? подсказки · / команды", "? shortcuts · / commands");
    let (right, right_style) = footer_right_segment(app);
    let width = area.width as usize;
    // Держим запас у правой стены и НЕ дорисовываем до последней колонки. Рендер печатает
    // строку по НАШЕЙ ширине (unicode-width считает `→`/`·` за 1 клетку), но терминал
    // рисует такие «неоднозначные по ширине» символы в 2 клетки, плюс крайняя ячейка
    // страдает от last-column-quirk — и хвост правого сегмента срезался у самой стены.
    // Запас в 2 колонки это покрывает (в сегменте максимум 2 таких символа с подсказками).
    let budget = width.saturating_sub(2);

    let mode_width = mode_label.chars().count();
    let switch_width = switch.chars().count() + 1; // пробел перед серым хоткеем
    let sep_width = 2;
    let min_gap = 2;

    // Правый сегмент ограничиваем доступным местом; не влезает — усекаем с «…», а не
    // молча теряем символ у края.
    let right_available = budget.saturating_sub(mode_width + switch_width + sep_width + min_gap);
    let right_slot_width = footer_right_slot_width(app).min(right_available);
    let right = truncate_chars(&right, right_slot_width);
    let right_width = display_width(&right);

    let used = mode_width + switch_width + sep_width + right_slot_width + min_gap;
    let hints = if used + hints.chars().count() > budget {
        truncate_chars(hints, budget.saturating_sub(used))
    } else {
        hints.to_string()
    };
    let left_width = mode_width + switch_width + sep_width + hints.chars().count();
    let gap = budget.saturating_sub(left_width + right_slot_width);
    let right_padding = right_slot_width.saturating_sub(right_width);
    let line = Line::from(vec![
        Span::styled(
            mode_label,
            Style::default()
                .fg(app.chat_mode.color())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(switch, Style::default().fg(MUTED)),
        Span::raw("  "),
        Span::styled(hints, Style::default().fg(app.theme.accent_soft())),
        Span::raw(" ".repeat(gap)),
        Span::raw(" ".repeat(right_padding)),
        Span::styled(right, right_style),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

pub(crate) fn footer_right_segments(app: &App) -> Vec<String> {
    let lang = app.lang;
    let ready = lang.choose("готов", "ready");
    let mut segments = Vec::new();

    if app.status != ready {
        segments.push(format!(
            "{}: {}",
            lang.choose("статус", "status"),
            app.status
        ));
    }

    // Роли планирования: архитектор → ревьюер (одна модель, если совпадают). Отдельный
    // сегмент «mode» убран — он дублировал ровно эти же роли.
    let architect = app.mode.architect_provider();
    let reviewer = app.mode.reviewer_provider();
    let roles = if architect == reviewer {
        architect.title().to_string()
    } else {
        format!("{} → {}", architect.title(), reviewer.title())
    };
    segments.push(format!("{}: {}", lang.choose("роли", "roles"), roles));
    segments.push(format!(
        "{}: {}",
        lang.choose("чат", "chat"),
        app.direct_provider.title()
    ));
    segments.push(format!(
        "{}: {}",
        lang.choose("тема", "theme"),
        app.theme.title()
    ));
    segments.push(format!(
        "{}: {}",
        lang.choose("усилие", "effort"),
        app.human_effort_summary()
    ));
    if app.usage.total_tokens() > 0 {
        segments.push(format!(
            "{}: {} · ${:.3}",
            lang.choose("расход", "usage"),
            format_token_count(app.usage.total_tokens() as usize),
            app.usage.total_cost_usd()
        ));
    }
    segments
}

pub(crate) fn footer_right_target(app: &App) -> String {
    let segments = footer_right_segments(app);

    let phase = rotating_phase(8, segments.len());
    segments.get(phase).cloned().unwrap_or_default()
}

pub(crate) fn footer_right_slot_width(app: &App) -> usize {
    let current_width = display_width(&app.footer_right_text);
    let previous_width = app
        .footer_right_previous_text
        .as_ref()
        .map(|previous| display_width(previous))
        .unwrap_or(0);

    current_width.max(previous_width)
}

pub(crate) fn footer_right_segment(app: &App) -> (String, Style) {
    let base_style = Style::default().fg(app.theme.accent_soft());
    let Some(changed_at) = app.footer_right_changed_at else {
        return (app.footer_right_text.clone(), base_style);
    };

    let elapsed_ms = changed_at.elapsed().as_millis();
    let previous = app
        .footer_right_previous_text
        .as_ref()
        .unwrap_or(&app.footer_right_text);

    if elapsed_ms < 360 {
        (
            previous.clone(),
            Style::default().fg(footer_transition_color(app.theme, elapsed_ms, false)),
        )
    } else {
        (
            app.footer_right_text.clone(),
            Style::default().fg(footer_transition_color(app.theme, elapsed_ms - 360, true)),
        )
    }
}

pub(crate) fn footer_transition_color(theme: Theme, elapsed_ms: u128, entering: bool) -> Color {
    let step = (elapsed_ms / 90).min(4) as usize;
    let palette = if entering {
        [
            theme.accent_dim(),
            Color::DarkGray,
            Color::Gray,
            theme.accent_soft(),
            theme.accent_soft(),
        ]
    } else {
        [
            theme.accent_soft(),
            Color::Gray,
            Color::DarkGray,
            theme.accent_dim(),
            theme.accent_dim(),
        ]
    };
    palette[step]
}
