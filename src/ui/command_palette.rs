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

    use ratatui::{backend::TestBackend, buffer::Buffer};

    // ── Часть 1. Цвета по уровню затухания (чистые функции) ──────────────────

    /// Каждый уровень — свой цвет, соседние различны (иначе удаление ветки match
    /// незаметно), а нулевой — не дефолт (`Default::default()` даёт Color::Reset).
    #[test]
    fn accent_colour_per_fade_level() {
        let theme = Theme::Purple;
        assert_eq!(command_palette_accent(theme, 0), Color::Indexed(236));
        assert_eq!(command_palette_accent(theme, 1), Color::Indexed(238));
        assert_eq!(command_palette_accent(theme, 2), Color::Indexed(240));
        assert_eq!(command_palette_accent(theme, 3), theme.accent_dim());
        assert_eq!(command_palette_accent(theme, 4), theme.accent());
        assert_eq!(command_palette_accent(theme, 5), theme.accent_soft());
        assert_eq!(command_palette_accent(theme, 6), theme.accent_soft());
        assert_ne!(command_palette_accent(theme, 0), Color::default());

        for level in 0..5 {
            assert_ne!(
                command_palette_accent(theme, level),
                command_palette_accent(theme, level + 1),
                "уровни accent {level} и {} совпали",
                level + 1
            );
        }
    }

    #[test]
    fn muted_colour_per_fade_level() {
        assert_eq!(command_palette_muted(0), Color::Indexed(236));
        assert_eq!(command_palette_muted(1), Color::Indexed(238));
        assert_eq!(command_palette_muted(2), Color::Indexed(240));
        assert_eq!(command_palette_muted(3), Color::Indexed(243));
        assert_eq!(command_palette_muted(4), Color::Indexed(246));
        assert_eq!(command_palette_muted(5), MUTED);
        assert_eq!(command_palette_muted(6), MUTED);
        assert_ne!(command_palette_muted(0), Color::default());

        for level in 0..5 {
            assert_ne!(
                command_palette_muted(level),
                command_palette_muted(level + 1),
                "уровни muted {level} и {} совпали",
                level + 1
            );
        }
    }

    #[test]
    fn selected_bg_per_fade_level() {
        let theme = Theme::Purple;
        assert_eq!(command_palette_selected_bg(theme, 0), None);
        assert_eq!(command_palette_selected_bg(theme, 1), None);
        assert_eq!(command_palette_selected_bg(theme, 2), None);
        assert_eq!(
            command_palette_selected_bg(theme, 3),
            Some(Color::Indexed(236))
        );
        assert_eq!(
            command_palette_selected_bg(theme, 4),
            Some(Color::Indexed(238))
        );
        assert_eq!(
            command_palette_selected_bg(theme, 5),
            Some(theme.accent_bg())
        );
        assert_eq!(
            command_palette_selected_bg(theme, 6),
            Some(theme.accent_bg())
        );
    }

    // ── Часть 2. Затухание по времени ────────────────────────────────────────

    /// None → финальный уровень 7 (бьёт замены `-> 0` и `-> 1`); свежее открытие
    /// → 0 (elapsed≈0); средний возраст 135 мс → ровно 3 (`/` = 3, `%` дал бы
    /// 135%45=0, `*` — огромное →7); далёкое прошлое 500 мс → кламп `.min(7)`.
    /// Граница 3→4 лежит на 180 мс, зазор от 135 достаточный — не флейкует.
    #[test]
    fn fade_level_grows_with_time_and_clamps() {
        let mut app = palette_app();

        app.command_palette_opened_at = None;
        assert_eq!(command_palette_fade_level(&app), 7);

        app.command_palette_opened_at = Some(Instant::now());
        assert_eq!(command_palette_fade_level(&app), 0);

        app.command_palette_opened_at = Some(Instant::now() - Duration::from_millis(135));
        assert_eq!(command_palette_fade_level(&app), 3);

        app.command_palette_opened_at = Some(Instant::now() - Duration::from_millis(500));
        assert_eq!(command_palette_fade_level(&app), 7);
    }

    // ── Часть 3. Экран палитры ───────────────────────────────────────────────

    fn palette_app() -> App {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = std::env::temp_dir().join(format!(
            "clave-palette-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::create_dir_all(&dir);
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
        app.theme = Theme::Purple;
        app
    }

    fn screen(app: &App, width: u16, height: u16) -> Buffer {
        let mut terminal =
            Terminal::new(TestBackend::new(width, height)).expect("оффскрин-терминал");
        terminal
            .draw(|frame| draw_command_screen(frame, frame.area(), app))
            .expect("отрисовка экрана");
        terminal.backend().buffer().clone()
    }

    fn buffer_rows(buffer: &Buffer) -> Vec<String> {
        let area = buffer.area;
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .filter_map(|x| buffer.cell((x, y)))
                    .map(|cell| cell.symbol())
                    .collect::<String>()
            })
            .collect()
    }

    /// Маркер «› » стоит на выбранной строке, у остальных — два пробела.
    /// Инверсия `command_index == selected` уводит маркер на чужую строку;
    /// инверсия `area.height == 0` при height 12 даёт ранний возврат — экран
    /// пуст, и оба `starts_with` падают.
    #[test]
    fn screen_marks_the_selected_command() {
        let mut app = palette_app();
        app.input = "/".to_string();
        app.selected_suggestion = 0;

        let rows = buffer_rows(&screen(&app, 80, 12));

        assert!(
            rows[0].starts_with("› "),
            "у выбранной строки нет маркера: {:?}",
            rows[0]
        );
        assert!(
            rows[1].starts_with("  "),
            "невыбранная строка получила маркер: {:?}",
            rows[1]
        );
        assert!(
            rows[1].trim_start().starts_with('/'),
            "во второй строке нет команды: {:?}",
            rows[1]
        );
    }

    /// Пустой список честно говорит «команд нет» — на обоих языках.
    /// Замена тела `draw_command_screen` на `()` оставила бы экран пустым.
    #[test]
    fn screen_reports_no_matching_commands_in_both_languages() {
        let mut app = palette_app();
        app.input = "/zzzznonexistent".to_string();
        assert!(app.suggestions().is_empty(), "фильтр обязан быть пустым");

        app.lang = Language::Ru;
        let ru = buffer_rows(&screen(&app, 80, 12)).join("\n");
        assert!(
            ru.contains("Команды не найдены"),
            "нет русского текста:\n{ru}"
        );

        app.lang = Language::En;
        let en = buffer_rows(&screen(&app, 80, 12)).join("\n");
        assert!(
            en.contains("No matching commands"),
            "нет английского текста:\n{en}"
        );
    }

    /// Инвариант: нулевая высота не валит кадр (буфер пуст, паники нет).
    /// Мутанта `area.height == 0`→`!=` он не убивает — это делает
    /// `screen_marks_the_selected_command` на height 12.
    #[test]
    fn screen_survives_zero_height() {
        let mut app = palette_app();
        app.input = "/".to_string();
        assert!(buffer_rows(&screen(&app, 80, 0)).is_empty());
    }

    /// Выбранная строка белеет ТОЛЬКО при высоком затухании (fade ≥ 5).
    /// `>=`→`<` (usage, стр. 93 и desc, стр. 107) инвертирует порог: при
    /// высоком fade даст цвет темы вместо белого, при низком — наоборот.
    #[test]
    fn selected_row_turns_white_only_at_high_fade() {
        let theme = Theme::Purple;

        let mut hi = palette_app();
        hi.theme = theme;
        hi.input = "/".to_string();
        hi.selected_suggestion = 0;
        hi.command_palette_opened_at = None; // fade_level == 7
        assert_eq!(command_palette_fade_level(&hi), 7);

        let buffer = screen(&hi, 80, 12);
        let rows = buffer_rows(&buffer);
        let y = rows
            .iter()
            .position(|r| r.starts_with("› "))
            .expect("выбранная строка") as u16;
        assert_eq!(
            buffer.cell((2, y)).unwrap().fg,
            Color::White,
            "usage при fade 7 — белый"
        );
        assert_eq!(
            buffer.cell((34, y)).unwrap().fg,
            Color::White,
            "описание при fade 7 — белое"
        );

        let mut lo = palette_app();
        lo.theme = theme;
        lo.input = "/".to_string();
        lo.selected_suggestion = 0;
        lo.command_palette_opened_at = Some(Instant::now()); // свежее открытие → низкий fade
        let fade = command_palette_fade_level(&lo);
        assert!(
            fade < 5,
            "свежая палитра должна давать низкий fade, а не {fade}"
        );

        let buffer = screen(&lo, 80, 12);
        let rows = buffer_rows(&buffer);
        let y = rows
            .iter()
            .position(|r| r.starts_with("› "))
            .expect("выбранная строка") as u16;
        assert_eq!(
            buffer.cell((2, y)).unwrap().fg,
            command_palette_accent(theme, fade)
        );
        assert_ne!(buffer.cell((2, y)).unwrap().fg, Color::White);
        assert_eq!(
            buffer.cell((34, y)).unwrap().fg,
            command_palette_muted(fade)
        );
        assert_ne!(buffer.cell((34, y)).unwrap().fg, Color::White);
    }
}
