use super::*;

pub(crate) fn draw_prompt_bar(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let lines = input_lines_wrapped(&app.input, area.width);
    let command_mode = normalized_command_query(&app.input).is_some();
    let tick = current_effort_tick();
    let (cursor_line, col) = input_cursor_position_wrapped(&app.input, app.cursor, area.width);

    // Область под сам ввод = высота композера минус две полоски. Когда строк ввода
    // больше (многострочная вставка), прокручиваем окно так, чтобы строка курсора
    // всегда оставалась видимой. Иначе курсор «отрывался» от текста, а хвост ввода
    // просто не показывался.
    let visible = area.height.saturating_sub(2).max(1) as usize;
    let scroll = cursor_line.saturating_sub(visible.saturating_sub(1));

    let mut rendered = Vec::new();
    // Верхняя полоска композера со встроенной у правого края плашкой названия чата.
    rendered.push(prompt_top_rule_line(area.width, command_mode, tick, app));
    for (offset, line) in lines.iter().skip(scroll).take(visible).enumerate() {
        // Маркер «›» — только на самой первой строке ВВОДА, даже если она прокручена вверх.
        let prefix = if scroll + offset == 0 { "› " } else { "  " };
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

    // +1: над первой видимой строкой ввода только верхняя полоска (плашка встроена в неё).
    // Курсор считаем ОТНОСИТЕЛЬНО окна прокрутки, поэтому он всегда в видимой области.
    let cursor_y = area.y + 1 + (cursor_line - scroll) as u16;
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, buffer::Buffer};

    // ── хелперы ──────────────────────────────────────────────────────────────

    fn line_text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    /// App на своих временных путях. `onboarding_done: true` — иначе поднимется
    /// auth-probe, и тест начнёт зависеть от установленных claude/codex (на CI их нет).
    fn prompt_app() -> App {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = std::env::temp_dir().join(format!(
            "clave-prompt-ui-{}-{}",
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

    /// Рисует композер в оффскрин-терминал и возвращает буфер и позицию курсора.
    /// Курсор берём из терминала: `draw` переносит его из кадра в backend.
    fn draw_into(app: &App, area: Rect, term_w: u16, term_h: u16) -> (Buffer, Position) {
        let mut terminal =
            Terminal::new(TestBackend::new(term_w, term_h)).expect("оффскрин-терминал");
        terminal
            .draw(|f| draw_prompt_bar(f, area, app))
            .expect("отрисовка композера");
        let cursor = terminal.get_cursor_position().expect("позиция курсора");
        (terminal.backend().buffer().clone(), cursor)
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

    // ── Прокрутка композера ──────────────────────────────────────────────────

    /// Многострочный ввод (вставка) выше композера: строка курсора ОБЯЗАНА оставаться
    /// видимой за счёт вертикальной прокрутки окна. Раньше скролла не было — курсор
    /// «отрывался» от текста, а хвост ввода не показывался.
    #[test]
    fn composer_scrolls_to_keep_the_cursor_line_visible() {
        let mut app = prompt_app();
        // 12 строк ввода; курсор в самом конце (на последней, L11).
        app.input = (0..12)
            .map(|i| format!("L{i:02}"))
            .collect::<Vec<_>>()
            .join("\n");
        app.cursor = app.input.len();

        let area = Rect {
            x: 0,
            y: 0,
            width: 40,
            height: 10,
        };
        let (buffer, cursor) = draw_into(&app, area, 40, 10);
        let rows = buffer_rows(&buffer);

        assert!(
            rows.iter().any(|r| r.contains("L11")),
            "строка курсора обязана быть видна:\n{}",
            rows.join("\n")
        );
        assert!(
            (cursor.y as usize) < rows.len() && rows[cursor.y as usize].contains("L11"),
            "курсор стоит на своей (последней) строке, а не съехал"
        );
        assert!(
            !rows.iter().any(|r| r.contains("L00")),
            "начало ввода прокручено вверх, а не залезло на курсор"
        );
    }

    // ── Часть 1. top_rule_line_with_title ────────────────────────────────────

    /// Плашка встроена у правого края: слева — линия, справа — ровно RIGHT_TAIL=2.
    /// Съедет left_len или хвост — плашка задвоится/уползёт, и суммарная ширина уйдёт.
    #[test]
    fn top_rule_embeds_the_badge_flush_to_the_right() {
        let line = top_rule_line_with_title(80, false, 0, Theme::Purple, "chat");

        // 72 столбца линии + плашка + 2 столбца хвоста (badge_len=6, left_len=72).
        assert_eq!(line.spans.len(), 75, "72 столбца + плашка + 2 хвоста");
        let badge = &line.spans[72];
        assert_eq!(
            badge.content.as_ref(),
            " chat ",
            "плашка стоит ровно на left_len=72"
        );
        assert_eq!(badge.style.fg, Some(Color::Black));
        assert_eq!(badge.style.bg, Some(Theme::Purple.accent()));
        assert!(badge.style.add_modifier.contains(Modifier::BOLD));

        assert!(line.spans[..72].iter().all(|s| s.content.as_ref() == "─"));
        assert_eq!(line.spans[73].content.as_ref(), "─", "хвост столбец 1");
        assert_eq!(line.spans[74].content.as_ref(), "─", "хвост столбец 2");

        let sum: usize = line
            .spans
            .iter()
            .map(|s| display_width(s.content.as_ref()))
            .sum();
        assert_eq!(sum, 80, "полоска обязана быть ровно шириной с терминал");

        let chars: Vec<char> = line_text(&line).chars().collect();
        assert_eq!(chars.len(), 80);
        assert_eq!(chars[72], ' ', "плашка начинается с внутреннего пробела");
        assert_eq!(chars[72..78].iter().collect::<String>(), " chat ");
        assert_eq!(chars[78], '─');
        assert_eq!(chars[79], '─');
    }

    /// Ширина полоски с плашкой всегда равна ширине терминала — на любой ширине.
    /// Уедет арифметика left_len/хвоста — сумма столбцов разойдётся с total.
    #[test]
    fn top_rule_width_always_equals_total() {
        for width in [8u16, 20, 40, 80, 120] {
            let line = top_rule_line_with_title(width, false, 0, Theme::Purple, "чат");
            let sum: usize = line
                .spans
                .iter()
                .map(|s| display_width(s.content.as_ref()))
                .sum();
            assert_eq!(sum, width as usize, "ширина разошлась при width={width}");
        }
    }

    /// Длинный заголовок режется по бюджету title_room = width-6, но линия остаётся
    /// шириной с терминал. Съедет вычитание/скобки — либо ширина уедет, либо длина текста.
    #[test]
    fn top_rule_truncates_a_long_title_to_the_room() {
        let title = "x".repeat(100);
        let line = top_rule_line_with_title(80, false, 0, Theme::Purple, &title);

        let sum: usize = line
            .spans
            .iter()
            .map(|s| display_width(s.content.as_ref()))
            .sum();
        assert_eq!(sum, 80, "обрезка не должна ломать общую ширину");

        let badge = line
            .spans
            .iter()
            .find(|s| s.style.bg == Some(Theme::Purple.accent()))
            .expect("плашка на фоне акцента");
        assert_eq!(
            badge.content.trim().chars().count(),
            74,
            "заголовок урезан по title_room = width - 6"
        );
    }

    /// Пустой заголовок — обычная линия без плашки (ветка title.is_empty()).
    /// `||`→`&&` заставило бы строить плашку из пустого текста — появился бы фон-акцент.
    #[test]
    fn top_rule_without_title_is_a_plain_line() {
        let line = top_rule_line_with_title(80, false, 0, Theme::Purple, "");
        assert_eq!(line_text(&line).chars().count(), 80);
        assert!(line_text(&line).chars().all(|c| c == '─'), "сплошная линия");
        assert!(
            line.spans.iter().all(|s| s.style.bg.is_none()),
            "у безымянного чата плашки нет"
        );
    }

    /// Ниже границы (total=6 < 7) — фолбэк без плашки. Держит `<→==`, `<→>` и сдвиги порога вниз.
    #[test]
    fn top_rule_below_the_min_width_falls_back() {
        let line = top_rule_line_with_title(6, false, 0, Theme::Purple, "Z");
        assert_eq!(line_text(&line).chars().count(), 6);
        assert!(
            line.spans.iter().all(|s| s.style.bg.is_none()),
            "на узкой линии плашки быть не должно"
        );
    }

    /// Ровно на границе (total=7) плашка уже помещается. Держит `<→==`, `<→<=` и сдвиги порога вверх.
    #[test]
    fn top_rule_at_the_min_width_shows_the_badge() {
        let line = top_rule_line_with_title(7, false, 0, Theme::Purple, "Z");
        assert_eq!(line_text(&line).chars().count(), 7);
        let badge = line
            .spans
            .iter()
            .find(|s| s.content.contains('Z'))
            .expect("на границе ширины плашка обязана быть");
        assert_eq!(badge.style.bg, Some(Theme::Purple.accent()));
    }

    // ── Часть 2. rule_span ───────────────────────────────────────────────────

    /// Цвет одного столбца линии. Неактивный — серый; активный переливается по (index+tick)%6.
    /// Пропажа любой ветки match или сдвиг фазы гасят/рвут переливание — сверяем точные цвета.
    #[test]
    fn rule_span_paints_the_column_by_phase() {
        let dim = Theme::Purple.accent_dim();
        let accent = Theme::Purple.accent();
        let soft = Theme::Purple.accent_soft();
        assert_ne!(dim, accent, "соседние цвета обязаны различаться");
        assert_ne!(accent, soft);
        assert_ne!(dim, soft);

        // Неактивный столбец — всегда серый, символ '─' (держит `delete !` и `-> Default`).
        for (index, tick) in [(0usize, 0u64), (1, 0), (2, 3), (5, 7)] {
            let s = rule_span(index, false, tick, Theme::Purple);
            assert_eq!(s.content.as_ref(), "─");
            assert_eq!(s.style.fg, Some(dim), "серый при index={index} tick={tick}");
        }

        // Активные фазы при tick=0 (index == фаза): держат удаление веток match.
        assert_eq!(rule_span(0, true, 0, Theme::Purple).style.fg, Some(dim));
        assert_eq!(rule_span(1, true, 0, Theme::Purple).style.fg, Some(accent));
        assert_eq!(rule_span(2, true, 0, Theme::Purple).style.fg, Some(soft));
        assert_eq!(rule_span(3, true, 0, Theme::Purple).style.fg, Some(soft));
        assert_eq!(rule_span(4, true, 0, Theme::Purple).style.fg, Some(soft));
        assert_eq!(rule_span(5, true, 0, Theme::Purple).style.fg, Some(accent));
        assert_eq!(rule_span(2, true, 0, Theme::Purple).content.as_ref(), "─");

        // Сдвиг фазы от tick: `+`→`-`/`*` и `%`→`/`/`+` дают другой остаток → другой цвет.
        assert_eq!(rule_span(5, true, 2, Theme::Purple).style.fg, Some(accent)); // (5+2)%6=1
        assert_eq!(rule_span(0, true, 6, Theme::Purple).style.fg, Some(dim)); // (0+6)%6=0

        // Цикл по модулю 6: цвет при tick и tick+6 совпадает.
        assert_eq!(
            rule_span(0, true, 0, Theme::Purple).style.fg,
            rule_span(0, true, 6, Theme::Purple).style.fg
        );
    }

    // ── Часть 3. prompt_rule_line / prompt_top_rule_line ─────────────────────

    /// Неактивная полоска — один сплошной серый спан; активная — столбец на символ, с цветами.
    /// `delete !` перепутал бы режимы, `-> Default` дал бы пустую линию.
    #[test]
    fn prompt_rule_line_is_solid_when_idle_and_shimmers_when_active() {
        let idle = prompt_rule_line(80, false, 0, Theme::Purple);
        assert_eq!(idle.spans.len(), 1, "неактивная — один сплошной спан");
        assert_eq!(line_text(&idle), "─".repeat(80));
        assert_eq!(idle.style.fg, Some(Theme::Purple.accent_dim()));

        let live = prompt_rule_line(80, true, 0, Theme::Purple);
        assert_eq!(live.spans.len(), 80, "активная — столбец на символ");
        assert_eq!(line_text(&live).chars().count(), 80);
        assert!(
            live.spans
                .iter()
                .any(|s| s.style.fg == Some(Theme::Purple.accent())),
            "есть акцентные столбцы"
        );
        assert!(
            live.spans
                .iter()
                .any(|s| s.style.fg == Some(Theme::Purple.accent_soft())),
            "есть мягкие столбцы"
        );
    }

    /// Плашка — только у явно названного чата (chat_title_custom). `-> Default` дал бы пустую линию.
    #[test]
    fn prompt_top_rule_line_shows_the_badge_only_for_a_named_chat() {
        let mut app = prompt_app();
        app.chat_title_custom = true;
        app.chat_title = "Имя".to_string();
        let line = prompt_top_rule_line(80, false, 0, &app);
        assert_eq!(line_text(&line).chars().count(), 80);
        let badge = line
            .spans
            .iter()
            .find(|s| s.content.contains("Имя"))
            .expect("плашка с названием чата");
        assert_eq!(badge.style.bg, Some(Theme::Purple.accent()));

        app.chat_title_custom = false;
        let plain = prompt_top_rule_line(80, false, 0, &app);
        assert_eq!(line_text(&plain).chars().count(), 80);
        assert!(
            plain.spans.iter().all(|s| s.style.bg.is_none()),
            "у безымянного чата плашки нет"
        );
    }

    // ── Часть 4. draw_prompt_bar + курсор ────────────────────────────────────

    /// Композер рисует полоски и ввод. `draw_prompt_bar -> ()` оставил бы пустой экран.
    #[test]
    fn draw_prompt_bar_renders_rules_and_input() {
        let mut app = prompt_app();
        app.input = "привет".to_string();
        app.cursor = app.input.len();

        let (buf, _) = draw_into(&app, Rect::new(0, 0, 30, 3), 30, 3);
        let rows = buffer_rows(&buf);
        assert!(
            rows.iter().any(|r| r.contains('─')),
            "полоска композера пропала"
        );
        assert!(
            rows.iter().any(|r| r.contains("привет")),
            "ввод не отрисован"
        );
        assert!(rows.iter().any(|r| r.contains("› ")), "нет префикса ввода");
    }

    /// Стрелка «›» — только у первой строки ввода, продолжение идёт с «  ».
    /// `index == 0` → `!=` перевесил бы стрелку на строки-продолжения.
    /// Многострочность — через перенос по ширине (content_width = width-2 = 18).
    #[test]
    fn draw_prompt_bar_marks_only_the_first_input_row() {
        let mut app = prompt_app();
        app.chat_title_custom = false;
        app.input = "a".repeat(25); // 18 в первой строке + 7 в переносе
        app.cursor = app.input.len();

        let (buf, _) = draw_into(&app, Rect::new(0, 0, 20, 4), 20, 4);
        let rows = buffer_rows(&buf);
        // y0 верхняя полоска, y1 первая строка ввода, y2 перенос, y3 нижняя полоска.
        assert!(
            rows[1].starts_with("› a"),
            "первая строка — со стрелкой: {:?}",
            rows[1]
        );
        assert!(
            rows[2].starts_with("  a"),
            "продолжение — без стрелки: {:?}",
            rows[2]
        );
    }

    /// Курсор встаёт под введённый символ: cursor_x = area.x + 2 + col, cursor_y = area.y + 1 + line.
    /// Area не в нуле, чтобы `*`/`-` дали заведомо другие координаты.
    #[test]
    fn draw_prompt_bar_places_the_cursor_under_the_input() {
        let mut app = prompt_app();
        app.input = "abc".to_string();
        app.cursor = 3;

        let (_, cur) = draw_into(&app, Rect::new(5, 3, 40, 6), 60, 12);
        // col=3, line=0 → x = 5+2+3 = 10, y = 3+1+0 = 4.
        assert_eq!((cur.x, cur.y), (10, 4), "курсор под последним символом");
    }

    /// На второй визуальной строке курсор съезжает вниз ровно на одну строку.
    /// Держит арифметику cursor_y при line_index > 0 (без него `-`/`*` были бы неотличимы).
    /// Перенос — по ширине: content_width = 40-2 = 38, ввод в 40 символов рвётся на две строки.
    #[test]
    fn draw_prompt_bar_moves_the_cursor_to_the_second_row() {
        let mut app = prompt_app();
        app.input = "a".repeat(40); // 38 в первой строке + 2 в переносе
        app.cursor = app.input.len(); // col=2, line=1

        let (_, cur) = draw_into(&app, Rect::new(5, 3, 40, 6), 60, 12);
        // col=2, line=1 → x = 5+2+2 = 9, y = 3+1+1 = 5.
        assert_eq!((cur.x, cur.y), (9, 5), "курсор на второй строке ввода");
    }

    /// Курсор прижат к правой границе area (max_x). `+`→`*` в max_x раздвинул бы границу.
    #[test]
    fn draw_prompt_bar_clamps_the_cursor_to_the_right_edge() {
        let mut app = prompt_app();
        app.input = "a".repeat(18);
        app.cursor = app.input.len(); // col=18, без переноса (content_width=18)

        let (_, cur) = draw_into(&app, Rect::new(3, 2, 20, 5), 30, 10);
        // cursor_x_raw = 3+2+18 = 23; max_x = 3 + (20-1) = 22 → прижат к 22.
        assert_eq!(cur.x, 22, "курсор не должен уходить за правую границу area");
    }

    /// В командном режиме нижняя полоска идёт с фазой tick+3 относительно верхней:
    /// столбец i снизу совпадает по цвету со столбцом i+3 сверху (один tick на кадр).
    /// `+`→`*` ломает этот сдвиг. (`+`→`-` эквивалентен: -3 ≡ +3 mod 6.)
    #[test]
    fn draw_prompt_bar_shifts_the_bottom_rule_phase_by_three() {
        let mut app = prompt_app();
        app.chat_title_custom = false;
        app.input = "/help".to_string();
        app.cursor = app.input.len();

        let (buf, _) = draw_into(&app, Rect::new(0, 0, 30, 3), 30, 3);
        let top: Vec<Color> = (0..30).map(|x| buf.cell((x, 0)).unwrap().fg).collect();
        let bottom: Vec<Color> = (0..30).map(|x| buf.cell((x, 2)).unwrap().fg).collect();

        let distinct: std::collections::HashSet<_> = top.iter().collect();
        assert!(
            distinct.len() >= 2,
            "полоска обязана переливаться, иначе сдвиг не проверить"
        );

        for i in 0..(30 - 3) {
            assert_eq!(
                bottom[i],
                top[i + 3],
                "фаза нижней полоски не сдвинута на 3 в столбце {i}"
            );
        }
    }
}
