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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, buffer::Buffer};

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    fn sc_app() -> App {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = std::env::temp_dir().join(format!(
            "clave-shortcuts-{}-{}",
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

    fn draw_rows(app: &App, screen_w: u16, screen_h: u16, area: Rect) -> Vec<String> {
        let mut terminal =
            Terminal::new(TestBackend::new(screen_w, screen_h)).expect("оффскрин-терминал");
        terminal
            .draw(|frame| draw_shortcuts_panel(frame, area, app))
            .expect("отрисовка панели");
        buffer_rows(&terminal.backend().buffer().clone())
    }

    // ── Часть 1. column_height ────────────────────────────────────────────────

    /// Высота колонки: заголовок + пункты на каждую группу, плюс разделитель МЕЖДУ
    /// группами. Числа — литералы, посчитанные вручную по каталогу (3,2,4,3 пункта).
    #[test]
    fn column_height_counts_titles_items_and_separators() {
        // Весь каталог: (1+3)+(1+2)+(1+4)+(1+3) + 3 разделителя = 16 + 3 = 19.
        assert_eq!(column_height(SHORTCUT_GROUPS), 19);
        // Первые две группы: (1+3)+(1+2) + 1 разделитель = 7 + 1 = 8.
        assert_eq!(column_height(&SHORTCUT_GROUPS[..2]), 8);
        // Одна группа — разделителя нет (groups-1 = 0): 1 + 3 = 4.
        assert_eq!(column_height(&SHORTCUT_GROUPS[..1]), 4);
        // Две группы = сумма одиночных РОВНО плюс один разделитель между ними.
        let g0 = column_height(&SHORTCUT_GROUPS[0..1]);
        let g1 = column_height(&SHORTCUT_GROUPS[1..2]);
        assert_eq!(column_height(&SHORTCUT_GROUPS[0..2]), g0 + g1 + 1);
    }

    // ── Часть 2. shortcut_split ───────────────────────────────────────────────

    /// Порог ровно 56: на нём — две колонки с границей по div_ceil(N/2); на 55 — одна.
    /// Каталог из 4 групп → 2/2; числа зафиксированы литералами.
    #[test]
    fn split_is_two_columns_at_56_and_one_below() {
        assert_eq!(SHORTCUT_GROUPS.len(), 4);

        let (left, right, two) = shortcut_split(56);
        assert!(two, "на 56 обязаны быть две колонки");
        assert!(!left.is_empty(), "левая колонка не пустая");
        assert_eq!(left.len(), 2);
        assert_eq!(right.len(), 2);
        assert_eq!(left.len() + right.len(), SHORTCUT_GROUPS.len());

        let (left, right, two) = shortcut_split(55);
        assert!(!two, "на 55 колонка одна");
        assert_eq!(left.len(), 4);
        assert!(right.is_empty());
    }

    // ── Часть 3. column_lines ─────────────────────────────────────────────────

    /// Заголовок каждой группы + пункты; между группами — РОВНО одна пустая строка,
    /// и её нет перед первой группой (index > 0).
    #[test]
    fn column_lines_render_titles_items_and_a_single_separator() {
        let groups = &SHORTCUT_GROUPS[..2];
        let lines = column_lines(groups, Language::Ru, Theme::Purple);

        assert_eq!(lines.len(), column_height(groups));
        assert_eq!(lines.len(), 8);

        // Первая строка — заголовок, НЕ пустая: разделителя перед ней нет.
        assert_eq!(line_text(&lines[0]), "Отправка");
        // Перед второй группой — ровно одна пустая строка.
        assert_eq!(line_text(&lines[4]), "");
        assert_eq!(line_text(&lines[5]), "Правка");

        let blanks = lines.iter().filter(|l| line_text(l).is_empty()).count();
        assert_eq!(blanks, 1, "разделитель обязан быть единственным");
    }

    /// Стили колонки: заголовок группы — BOLD + accent темы, клавиши пункта — BOLD,
    /// описание — MUTED. Съедет любой из них — панель потеряет визуальную структуру.
    #[test]
    fn column_lines_style_title_keys_and_description() {
        let lines = column_lines(&SHORTCUT_GROUPS[..1], Language::Ru, Theme::Purple);

        // lines[0] — заголовок «Отправка».
        let title = &lines[0].spans[0];
        assert_eq!(title.style.fg, Some(Theme::Purple.accent()));
        assert!(title.style.add_modifier.contains(Modifier::BOLD));

        // lines[1] — первый пункт: span[0] клавиши BOLD, span[1] описание MUTED.
        let keys = &lines[1].spans[0];
        assert!(keys.style.add_modifier.contains(Modifier::BOLD));
        let desc = &lines[1].spans[1];
        assert_eq!(desc.style.fg, Some(MUTED));
    }

    /// Поле клавиш в колонке — фиксированной ширины по самой длинной клавише:
    /// короткие клавиши добиты пробелами, столбик описаний не разъезжается.
    #[test]
    fn column_lines_align_keys_into_a_fixed_column() {
        let groups = &SHORTCUT_GROUPS[..1]; // Enter, Shift/Alt+Enter, Tab
        let lines = column_lines(groups, Language::Ru, Theme::Purple);

        let key_widths: Vec<usize> = lines[1..]
            .iter()
            .map(|l| l.spans[0].content.chars().count())
            .collect();
        assert!(
            key_widths.iter().all(|w| *w == key_widths[0]),
            "поле клавиш разъехалось: {key_widths:?}"
        );
        // Поле = 2 пробела + самая длинная клавиша (15) + 2 пробела = 19.
        assert_eq!(key_widths[0], 2 + "Shift/Alt+Enter".chars().count() + 2);
    }

    // ── Часть 4. shortcuts_panel_height ───────────────────────────────────────

    /// Итоговая высота — макс из колонок под клампом [3,14], никогда 0/1.
    /// ВЕРХНИЙ кламп проверяется явно (19 → 14). НИЖНИЙ кламп (до 3) на текущем
    /// каталоге недостижим: любая раскладка ≥ 8 строк, так что мутанты нижней границы
    /// clamp здесь непокрываемы без подмены данных (в списке 21 их нет).
    #[test]
    fn panel_height_maxes_columns_and_clamps() {
        // Широкий → две колонки max(8,10)=10, внутри диапазона.
        assert_eq!(shortcuts_panel_height(80), 10);
        // Узкий → одна колонка высотой 19, верхний кламп режет до 14.
        assert_eq!(shortcuts_panel_height(40), 14);

        for w in [0u16, 1, 10, 40, 55, 56, 80, 200] {
            let h = shortcuts_panel_height(w);
            assert!(
                (3..=14).contains(&h),
                "высота {h} вне [3,14] при ширине {w}"
            );
        }
    }

    // ── Часть 5. draw_shortcuts_panel ─────────────────────────────────────────

    /// Широкий экран: РОВНО две колонки. Заголовок правой половины «Навигация» стоит
    /// в той же строке, что заголовок левой «Отправка» — в одноколоночной раскладке
    /// такого не бывает (группы идут стопкой). Так тест доказывает именно две колонки.
    #[test]
    fn wide_screen_shows_both_columns_side_by_side() {
        let app = sc_app();
        let rows = draw_rows(&app, 80, 20, Rect::new(0, 0, 80, 20));
        let same_row = rows
            .iter()
            .any(|r| r.contains("Отправка") && r.contains("Навигация"));
        assert!(
            same_row,
            "нет двух колонок в один ряд:\n{}",
            rows.join("\n")
        );
    }

    /// Узкий экран: одна колонка — все четыре группы стопкой.
    #[test]
    fn narrow_screen_stacks_all_groups() {
        let app = sc_app();
        let rows = draw_rows(&app, 40, 22, Rect::new(0, 0, 40, 22));
        let text = rows.join("\n");
        for title in ["Отправка", "Правка", "Навигация", "Сессия"] {
            assert!(text.contains(title), "нет группы «{title}»:\n{text}");
        }
    }

    /// Контент сдвинут на x+1: рисуем в область с x=2 → заголовок с колонки 3.
    #[test]
    fn panel_indents_content_by_one_column() {
        let app = sc_app();
        let rows = draw_rows(&app, 80, 20, Rect::new(2, 0, 78, 20));
        let row = rows
            .iter()
            .find(|r| r.contains("Отправка"))
            .expect("нет заголовка");
        let first = row.find(|c: char| c != ' ').expect("строка пустая");
        assert_eq!(first, 3, "левый отступ съехал: {row:?}");
    }

    /// Ширина ужимается на отступ: при экране 56 внутри остаётся 55 → одна колонка.
    /// Не ужать ширину — внутри 56 → две колонки, и «Навигация» встанет в один ряд
    /// с «Отправкой», чего в одноколоночной раскладке не бывает.
    #[test]
    fn panel_width_is_reduced_by_the_indent() {
        let app = sc_app();
        let rows = draw_rows(&app, 56, 22, Rect::new(0, 0, 56, 22));
        let nav_row = rows
            .iter()
            .find(|r| r.contains("Навигация"))
            .expect("нет Навигации");
        assert!(
            !nav_row.contains("Отправка"),
            "ширину не ужали — правая колонка появилась там, где не должна:\n{nav_row:?}"
        );
    }
}
