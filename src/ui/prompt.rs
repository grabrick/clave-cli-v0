use super::*;

pub(crate) fn draw_prompt_bar(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let lines = input_lines_wrapped(&app.input, area.width);
    let command_mode = normalized_command_query(&app.input).is_some();
    let tick = current_effort_tick();
    let mut rendered = Vec::new();

    // Верхняя полоска композера со встроенной у правого края плашкой названия чата.
    rendered.push(prompt_top_rule_line(area.width, command_mode, tick, app));
    for (index, line) in lines.iter().enumerate() {
        let prefix = if index == 0 { "› " } else { "  " };
        rendered.push(Line::from(vec![
            Span::styled(
                prefix,
                Style::default()
                    .fg(app.theme.accent())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(line.clone()),
        ]));
    }
    rendered.push(prompt_rule_line(
        area.width,
        command_mode,
        tick + 3,
        app.theme,
    ));

    frame.render_widget(Paragraph::new(rendered), area);

    let (line_index, col) = input_cursor_position_wrapped(&app.input, app.cursor, area.width);
    // +1: над первой строкой ввода только верхняя полоска (плашка встроена в неё).
    let cursor_y = area.y + 1 + (line_index as u16).min(area.height.saturating_sub(2));
    let cursor_x = area.x + 2 + col as u16;
    let max_x = area.x + area.width.saturating_sub(1);
    frame.set_cursor_position(Position::new(cursor_x.min(max_x), cursor_y));
}

/// Верхняя полоска композера. Плашка названия — ТОЛЬКО для явно названного чата
/// (/name, /rename); у безымянного (chat_id по умолчанию) рисуется чистая полоска.
fn prompt_top_rule_line(width: u16, active: bool, tick: u64, app: &App) -> Line<'static> {
    let title = if app.chat_title_custom {
        app.chat_title.as_str()
    } else {
        ""
    };
    top_rule_line_with_title(width, active, tick, app.theme, title)
}

/// Горизонтальная линия со встроенной у правого края плашкой `title`. После
/// плашки — короткий «хвост» из `─` до границы, слева — продолжение линии. Если
/// названия нет или ширины не хватает, рисуется обычная полоска без плашки.
/// Чистая функция (без `App`) — удобно покрыть тестом.
fn top_rule_line_with_title(
    width: u16,
    active: bool,
    tick: u64,
    theme: Theme,
    title: &str,
) -> Line<'static> {
    let total = width as usize;
    let title = title.trim();

    // Хвост из `─` справа от плашки и минимальный «островок» линии слева.
    const RIGHT_TAIL: usize = 2;
    const MIN_LEFT: usize = 2;

    if title.is_empty() || total < MIN_LEFT + RIGHT_TAIL + 3 {
        return prompt_rule_line(width, active, tick, theme);
    }

    // Бюджет под текст: минус 2 внутренних пробела плашки, хвост и островок слева.
    let title_room = total - (RIGHT_TAIL + MIN_LEFT + 2);
    let title = truncate_chars(title, title_room);
    let badge = format!(" {title} ");
    let badge_len = display_width(&badge);
    let left_len = total.saturating_sub(badge_len + RIGHT_TAIL);

    let mut spans = Vec::with_capacity(left_len + 1 + RIGHT_TAIL);
    for index in 0..left_len {
        spans.push(rule_span(index, active, tick, theme));
    }
    spans.push(Span::styled(
        badge,
        Style::default()
            .fg(Color::Black)
            .bg(theme.accent())
            .add_modifier(Modifier::BOLD),
    ));
    for index in (left_len + badge_len)..total {
        spans.push(rule_span(index, active, tick, theme));
    }
    Line::from(spans)
}

/// Один символ горизонтальной полоски для столбца `index`. В командном режиме
/// столбцы переливаются акцентными оттенками в зависимости от `tick`.
fn rule_span(index: usize, active: bool, tick: u64, theme: Theme) -> Span<'static> {
    if !active {
        return Span::styled("─", Style::default().fg(theme.accent_dim()));
    }
    let color = match ((index as u64 + tick) % 6) as usize {
        0 => theme.accent_dim(),
        1 | 5 => theme.accent(),
        2..=4 => theme.accent_soft(),
        _ => theme.accent(),
    };
    Span::styled("─", Style::default().fg(color))
}

pub(crate) fn prompt_rule_line(width: u16, active: bool, tick: u64, theme: Theme) -> Line<'static> {
    if !active {
        return Line::styled(
            "─".repeat(width as usize),
            Style::default().fg(theme.accent_dim()),
        );
    }

    let spans = (0..width as usize)
        .map(|index| rule_span(index, active, tick, theme))
        .collect::<Vec<_>>();
    Line::from(spans)
}
