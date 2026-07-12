use super::*;

/// Высота панели `?` (без рамки): столько строк, сколько в самой длинной колонке.
pub(crate) fn shortcuts_panel_height(width: u16) -> u16 {
    let (left, right, two_col) = shortcut_split(width);
    let rows = if two_col {
        column_height(left).max(column_height(right))
    } else {
        column_height(left)
    };
    (rows as u16).clamp(3, 14)
}

pub(crate) fn draw_shortcuts_panel(frame: &mut Frame<'_>, area: Rect, app: &App) {
    // Без обводки: контент рисуется прямо в область (как командная палитра), с небольшим
    // левым отступом. Структуру несут заголовки групп акцентом, а не рамка.
    let area = Rect {
        x: area.x + 1,
        width: area.width.saturating_sub(1),
        ..area
    };
    let (left, right, two_col) = shortcut_split(area.width);
    if two_col {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        frame.render_widget(
            Paragraph::new(column_lines(left, app.lang, app.theme)),
            cols[0],
        );
        frame.render_widget(
            Paragraph::new(column_lines(right, app.lang, app.theme)),
            cols[1],
        );
    } else {
        frame.render_widget(
            Paragraph::new(column_lines(left, app.lang, app.theme)),
            area,
        );
    }
}

/// Делит группы на колонки: две при широком экране, одна — при узком.
fn shortcut_split(width: u16) -> (&'static [ShortcutGroup], &'static [ShortcutGroup], bool) {
    if width >= 56 {
        let mid = SHORTCUT_GROUPS.len().div_ceil(2);
        let (left, right) = SHORTCUT_GROUPS.split_at(mid);
        (left, right, true)
    } else {
        (SHORTCUT_GROUPS, &[], false)
    }
}

fn column_height(groups: &[ShortcutGroup]) -> usize {
    // Заголовок + строки по каждой группе, плюс пустая строка между группами.
    groups.iter().map(|g| 1 + g.items.len()).sum::<usize>() + groups.len().saturating_sub(1)
}

/// Строки одной колонки: заголовки групп — акцентом, клавиши — жирным в выровненной
/// колонке, описания — приглушённым. Ширина колонки клавиш = самая длинная клавиша
/// именно в этой колонке (так короткие клавиши не тонут в отступах соседней колонки).
fn column_lines(groups: &[ShortcutGroup], lang: Language, theme: Theme) -> Vec<Line<'static>> {
    let key_w = groups
        .iter()
        .flat_map(|g| g.items)
        .map(|spec| display_width(spec.keys))
        .max()
        .unwrap_or(0);

    let mut lines = Vec::new();
    for (index, group) in groups.iter().enumerate() {
        if index > 0 {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            group.title(lang),
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD),
        )));
        for spec in group.items {
            let pad = " ".repeat(key_w.saturating_sub(display_width(spec.keys)));
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {}{}  ", spec.keys, pad),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(spec.describe(lang), Style::default().fg(MUTED)),
            ]));
        }
    }
    lines
}
