use super::*;

pub(crate) fn command_palette_fade_level(app: &App) -> usize {
    app.command_palette_opened_at
        .map(|opened_at| (opened_at.elapsed().as_millis() / 45).min(7) as usize)
        .unwrap_or(7)
}

pub(crate) fn command_palette_accent(theme: Theme, level: usize) -> Color {
    match level {
        0 => Color::Indexed(236),
        1 => Color::Indexed(238),
        2 => Color::Indexed(240),
        3 => theme.accent_dim(),
        4 => theme.accent(),
        _ => theme.accent_soft(),
    }
}

pub(crate) fn command_palette_muted(level: usize) -> Color {
    match level {
        0 => Color::Indexed(236),
        1 => Color::Indexed(238),
        2 => Color::Indexed(240),
        3 => Color::Indexed(243),
        4 => Color::Indexed(246),
        _ => MUTED,
    }
}

pub(crate) fn command_palette_selected_bg(theme: Theme, level: usize) -> Option<Color> {
    match level {
        0..=2 => None,
        3 => Some(Color::Indexed(236)),
        4 => Some(Color::Indexed(238)),
        _ => Some(theme.accent_bg()),
    }
}

pub(crate) fn command_palette_scroll_offset(
    selected: usize,
    visible_rows: usize,
    total_rows: usize,
) -> usize {
    if visible_rows == 0 || total_rows <= visible_rows {
        return 0;
    }

    selected
        .saturating_add(1)
        .saturating_sub(visible_rows)
        .min(total_rows.saturating_sub(visible_rows))
}

pub(crate) fn draw_command_screen(frame: &mut Frame<'_>, area: Rect, app: &App) {
    frame.render_widget(Clear, area);
    if area.height == 0 {
        return;
    }

    let suggestions = app.suggestions();
    let commands = suggestions;
    if commands.is_empty() {
        let line = Line::styled(
            app.lang
                .choose("Команды не найдены", "No matching commands"),
            Style::default().fg(command_palette_muted(command_palette_fade_level(app))),
        );
        frame.render_widget(Paragraph::new(vec![line]), area);
        return;
    }

    let selected = app
        .selected_suggestion
        .min(commands.len().saturating_sub(1));
    let visible = area.height as usize;
    let offset = command_palette_scroll_offset(selected, visible, commands.len());
    let fade_level = command_palette_fade_level(app);

    let lines = commands
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible)
        .map(|(command_index, command)| {
            let is_selected = command_index == selected;
            // Равномерное появление всей палитры: без позиционного затухания к низу
            // (раньше `visual_index / 3` навсегда гасил нижние строки — читалось как
            // «выключенные» команды).
            let row_fade = fade_level;
            let command_style = if is_selected {
                let mut style = Style::default()
                    .fg(if row_fade >= 5 {
                        Color::White
                    } else {
                        command_palette_accent(app.theme, row_fade)
                    })
                    .add_modifier(Modifier::BOLD);
                if let Some(bg) = command_palette_selected_bg(app.theme, row_fade) {
                    style = style.bg(bg);
                }
                style
            } else {
                Style::default().fg(command_palette_accent(app.theme, row_fade))
            };
            let desc_style = if is_selected {
                Style::default().fg(if row_fade >= 5 {
                    Color::White
                } else {
                    command_palette_muted(row_fade)
                })
            } else {
                Style::default().fg(command_palette_muted(row_fade))
            };

            Line::from(vec![
                Span::styled(
                    if is_selected { "› " } else { "  " },
                    Style::default().fg(command_palette_muted(row_fade)),
                ),
                Span::styled(format!("{:<30}", command.usage), command_style),
                Span::raw("  "),
                Span::styled(command.description(app.lang), desc_style),
            ])
        })
        .collect::<Vec<_>>();

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_offset_keeps_selected_row_visible() {
        assert_eq!(command_palette_scroll_offset(0, 12, 31), 0);
        assert_eq!(command_palette_scroll_offset(11, 12, 31), 0);
        assert_eq!(command_palette_scroll_offset(12, 12, 31), 1);
        assert_eq!(command_palette_scroll_offset(30, 12, 31), 19);
    }

    #[test]
    fn scroll_offset_handles_short_or_empty_lists() {
        assert_eq!(command_palette_scroll_offset(7, 12, 8), 0);
        assert_eq!(command_palette_scroll_offset(7, 0, 8), 0);
    }
}
