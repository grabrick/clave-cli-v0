use super::*;

/// Высота панели селектора: рамка + (степпер) + контент + подсказка.
pub(crate) fn ask_panel_height(state: &AskState, width: u16, cap: u16) -> u16 {
    let stepper = u16::from(state.multi_question());
    let iw = (width as usize).saturating_sub(2).max(8); // минус рамка
    let body = if state.on_confirm() {
        (state.confirm_rows() as u16).min(12)
    } else if let Some(question) = state.question() {
        // Высота с учётом переноса: строка(и) вопроса + варианты + «Свой ответ» (если он есть).
        let mut rows = wrapped_rows(display_width(&question.question), iw);
        for opt in &question.options {
            // ~6 символов на маркер/номер/чекбокс перед текстом варианта.
            rows += wrapped_rows(display_width(&opt.label) + 6, iw);
        }
        rows += usize::from(question.allow_custom);
        rows as u16
    } else {
        1
    };
    (2 + stepper + body + 1).min(cap).max(4)
}

/// Сколько визуальных строк займёт текст длиной `chars` при ширине `width`.
fn wrapped_rows(chars: usize, width: usize) -> usize {
    if width == 0 {
        return 1;
    }
    chars.max(1).div_ceil(width)
}

pub(crate) fn draw_ask_panel(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(state) = &app.ask else {
        return;
    };
    if area.height == 0 {
        return;
    }

    let color = app.theme.accent();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(Span::styled(
            app.lang.choose(" Выбор ", " Choose "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )))
        .border_style(Style::default().fg(color));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let iw = inner.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();

    // Степпер «Вопрос i/N · Подтверждение» — только при нескольких вопросах.
    if state.multi_question() {
        lines.push(stepper_line(state, app, color));
    }

    if state.on_confirm() {
        draw_confirm_rows(state, app, color, iw, inner.height, &mut lines);
    } else {
        draw_question_rows(state, app, color, iw, inner.height, &mut lines);
    }

    // Подсказка по клавишам — зависит от шага.
    lines.push(Line::styled(
        truncate_chars(ask_hint(state, app), iw),
        Style::default().fg(MUTED),
    ));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn stepper_line(state: &AskState, app: &App, color: Color) -> Line<'static> {
    let total = state.prompt.questions.len();
    let on_question = !state.on_confirm();
    let active = Style::default().fg(color).add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(MUTED);
    // На вопросе — «Вопрос i/N»; на подтверждении — «Вопросы» без номера (мы уже не
    // на конкретном вопросе), а подсвечен «Подтверждение».
    let questions_label = if on_question {
        format!(
            "{} {}/{total}",
            app.lang.choose("Вопрос", "Question"),
            state.step + 1
        )
    } else {
        app.lang.choose("Вопросы", "Questions").to_string()
    };
    Line::from(vec![
        Span::styled(questions_label, if on_question { active } else { dim }),
        Span::styled("  ·  ", dim),
        Span::styled(
            app.lang.choose("Подтверждение", "Confirm"),
            if on_question { dim } else { active },
        ),
    ])
}

fn draw_question_rows(
    state: &AskState,
    app: &App,
    color: Color,
    iw: usize,
    inner_h: u16,
    lines: &mut Vec<Line<'static>>,
) {
    let (Some(question), Some(answer)) = (state.question(), state.current_answer()) else {
        return;
    };
    // Текст вопроса (жирный, переносится при нехватке ширины — см. Paragraph::wrap).
    lines.push(Line::styled(
        question.question.clone(),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    ));

    let capacity = (inner_h as usize)
        .saturating_sub(lines.len() + 1) // уже занятое + строка подсказки
        .max(1);
    let total = question.options.len() + usize::from(question.allow_custom);
    let offset = command_palette_scroll_offset(answer.cursor, capacity, total);
    for idx in offset..(offset + capacity).min(total) {
        let selected = idx == answer.cursor;
        if idx < question.options.len() {
            let opt = &question.options[idx];
            let mut spans = vec![
                Span::styled(row_marker(selected), Style::default().fg(color)),
                // Нумерация пунктов — приглушённая, ярче на выбранном.
                Span::styled(
                    format!("{}. ", idx + 1),
                    Style::default().fg(if selected { color } else { MUTED }),
                ),
            ];
            if question.multi {
                let checked = answer.checked[idx];
                spans.push(Span::styled(
                    if checked { "[x] " } else { "[ ] " },
                    Style::default().fg(if checked { color } else { MUTED }),
                ));
            }
            spans.push(Span::styled(
                opt.label.clone(),
                if selected {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(app.theme.accent_soft())
                },
            ));
            if let Some(note) = &opt.note {
                let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
                let room = iw.saturating_sub(used + 3);
                if room > 4 {
                    spans.push(Span::styled(
                        format!(" — {}", truncate_chars(note, room)),
                        Style::default().fg(MUTED),
                    ));
                }
            }
            lines.push(Line::from(spans));
        } else {
            // «Свой ответ» — следующий по счёту номер после вариантов.
            lines.push(custom_field_line(
                answer,
                app,
                color,
                iw,
                selected,
                question.options.len() + 1,
            ));
        }
    }
}

/// Отступ строки-пункта: стрелка на выбранном, пробелы иначе. Один пробел слева
/// для воздуха, чтобы пункты были чуть отбиты от рамки и текста вопроса.
fn row_marker(selected: bool) -> &'static str {
    if selected {
        " › "
    } else {
        "   "
    }
}

fn custom_field_line(
    answer: &AnswerState,
    app: &App,
    color: Color,
    iw: usize,
    selected: bool,
    number: usize,
) -> Line<'static> {
    let label = app.lang.choose("Свой ответ: ", "Custom: ");
    let mut spans = vec![
        Span::styled(row_marker(selected), Style::default().fg(color)),
        Span::styled(
            format!("{number}. "),
            Style::default().fg(if selected { color } else { MUTED }),
        ),
        Span::styled(
            label,
            if selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(MUTED)
            },
        ),
    ];
    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let room = iw.saturating_sub(used + 1);
    if selected {
        if !answer.custom.is_empty() && room > 0 {
            let chars: Vec<char> = answer.custom.chars().collect();
            let shown: String = if chars.len() > room {
                chars[chars.len() - room..].iter().collect()
            } else {
                answer.custom.clone()
            };
            spans.push(Span::styled(shown, Style::default().fg(Color::White)));
        }
        spans.push(Span::styled("▌", Style::default().fg(color)));
    } else if answer.custom.is_empty() {
        spans.push(Span::styled(
            app.lang.choose("впишите свой вариант", "type your own"),
            Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
        ));
    } else {
        spans.push(Span::styled(
            truncate_chars(&answer.custom, room),
            Style::default().fg(MUTED),
        ));
    }
    Line::from(spans)
}

fn draw_confirm_rows(
    state: &AskState,
    app: &App,
    color: Color,
    iw: usize,
    inner_h: u16,
    lines: &mut Vec<Line<'static>>,
) {
    let total = state.confirm_rows(); // вопросы + «Отправить»
    let capacity = (inner_h as usize).saturating_sub(lines.len() + 1).max(1);
    let offset = command_palette_scroll_offset(state.confirm_cursor, capacity, total);
    let questions = state.prompt.questions.len();
    for idx in offset..(offset + capacity).min(total) {
        let selected = idx == state.confirm_cursor;
        let marker = row_marker(selected);
        if idx < questions {
            let chosen = state.chosen(idx);
            let answer = if chosen.is_empty() {
                app.lang.choose("—", "—").to_string()
            } else {
                chosen.join(", ")
            };
            // «N. вопрос: ответ» — вопрос приглушён, ответ ярче.
            let q_short = truncate_chars(&state.prompt.questions[idx].question, iw / 2);
            let prefix = format!("{marker}{}. {q_short}: ", idx + 1);
            let room = iw.saturating_sub(prefix.chars().count());
            lines.push(Line::from(vec![
                Span::styled(
                    prefix,
                    if selected {
                        Style::default().fg(Color::White)
                    } else {
                        Style::default().fg(MUTED)
                    },
                ),
                Span::styled(
                    truncate_chars(&answer, room.max(4)),
                    Style::default()
                        .fg(if selected {
                            Color::White
                        } else {
                            app.theme.accent_soft()
                        })
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(marker, Style::default().fg(color)),
                Span::styled(
                    app.lang.choose("Отправить ответы", "Send answers"),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
            ]));
        }
    }
}

fn ask_hint(state: &AskState, app: &App) -> &'static str {
    if state.on_confirm() {
        return app.lang.choose(
            "↑↓ выбрать · Enter правка/отправить · ←/Shift+Tab назад · Esc отмена",
            "↑↓ move · Enter edit/send · ←/Shift+Tab back · Esc cancel",
        );
    }
    let multi_q = state.multi_question();
    let multi_opt = state.question().is_some_and(|q| q.multi);
    match (multi_q, multi_opt) {
        (true, true) => app.lang.choose(
            "↑↓ · Space/Enter отметить · Tab дальше · Shift+Tab назад · Esc отмена",
            "↑↓ · Space/Enter toggle · Tab next · Shift+Tab back · Esc cancel",
        ),
        (true, false) => app.lang.choose(
            "↑↓ выбрать · Enter/Tab дальше · Shift+Tab назад · Esc отмена",
            "↑↓ move · Enter/Tab next · Shift+Tab back · Esc cancel",
        ),
        (false, true) => app.lang.choose(
            "↑↓ · Space отметить · Enter подтвердить · Esc отмена",
            "↑↓ · Space toggle · Enter confirm · Esc cancel",
        ),
        (false, false) => app.lang.choose(
            "↑↓ выбрать · Enter подтвердить · Esc отмена",
            "↑↓ move · Enter confirm · Esc cancel",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, buffer::Buffer};

    // ── общие хелперы ────────────────────────────────────────────────────────

    /// App на своих временных путях. `onboarding_done: true` — иначе поднимется
    /// auth-probe, и тест начнёт зависеть от установленных claude/codex (на CI их нет).
    fn ask_app() -> App {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = std::env::temp_dir().join(format!(
            "clave-ask-ui-{}-{}",
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

    fn opt(label: &str) -> AskOption {
        AskOption {
            label: label.to_string(),
            note: None,
        }
    }

    fn question(text: &str, multi: bool, labels: &[&str], allow_custom: bool) -> AskQuestion {
        AskQuestion {
            question: text.to_string(),
            multi,
            options: labels.iter().map(|l| opt(l)).collect(),
            allow_custom,
        }
    }

    fn answer(options: usize) -> AnswerState {
        AnswerState {
            cursor: 0,
            checked: vec![false; options],
            custom: String::new(),
        }
    }

    /// Состояние на шаге `step`. Курсоры/отметки/custom правим у полей после сборки.
    fn ask_state(questions: Vec<AskQuestion>, step: usize) -> AskState {
        let answers = questions.iter().map(|q| answer(q.options.len())).collect();
        AskState {
            prompt: AskPrompt { questions },
            answers,
            step,
            confirm_cursor: 0,
            feeds_tandem: false,
        }
    }

    fn custom_answer(text: &str) -> AnswerState {
        AnswerState {
            cursor: 0,
            checked: Vec::new(),
            custom: text.to_string(),
        }
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn span_texts(line: &Line<'_>) -> Vec<String> {
        line.spans.iter().map(|s| s.content.to_string()).collect()
    }

    fn has(lines: &[Line<'static>], needle: &str) -> bool {
        lines.iter().any(|l| line_text(l).contains(needle))
    }

    fn find<'a>(lines: &'a [Line<'static>], needle: &str) -> &'a Line<'static> {
        lines
            .iter()
            .find(|l| line_text(l).contains(needle))
            .unwrap_or_else(|| panic!("нет строки с «{needle}»"))
    }

    fn ask_screen(app: &App, w: u16, h: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("оффскрин-терминал");
        terminal
            .draw(|f| draw_ask_panel(f, f.area(), app))
            .expect("отрисовка панели");
        terminal.backend().buffer().clone()
    }

    fn buffer_rows(buffer: &Buffer) -> Vec<String> {
        let area = buffer.area;
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .filter_map(|x| buffer.cell((x, y)))
                    .map(|c| c.symbol())
                    .collect::<String>()
            })
            .collect()
    }

    // ── Часть 1. Чистая арифметика ───────────────────────────────────────────

    /// Высота переноса — точным числом. Нулевая ширина не делит на ноль (26:14),
    /// пустой текст — всё равно строка (нижняя граница max(1)).
    #[test]
    fn wrapped_rows_counts_visual_lines_and_survives_zero_width() {
        assert_eq!(wrapped_rows(0, 10), 1);
        assert_eq!(wrapped_rows(1, 10), 1);
        assert_eq!(wrapped_rows(10, 4), 3);
        assert_eq!(wrapped_rows(8, 4), 2);
        assert_eq!(wrapped_rows(5, 0), 1); // 26:14 == -> != упал бы в div_ceil(0)
        assert_eq!(wrapped_rows(20, 10), 2); // ≠1 → мутанты 26:5 -> 0/1 падают
    }

    /// Высота панели — литералами. Каждое слагаемое формулы (степпер, тело, рамка,
    /// подсказка) закрыто своим сравнением; сдвиг любого поменяет число.
    #[test]
    fn panel_height_is_frame_plus_stepper_plus_body() {
        // Один вопрос, 3 варианта, без «своего ответа»: тело = вопрос(1) + 3·вариант(1) = 4.
        let base = ask_state(vec![question("Q?", false, &["A", "B", "C"], false)], 0);
        assert_eq!(ask_panel_height(&base, 40, 100), 7);

        // +1 на строку «Свой ответ» (16:14 += ).
        let custom = ask_state(vec![question("Q?", false, &["A", "B", "C"], true)], 0);
        assert_eq!(ask_panel_height(&custom, 40, 100), 8);

        // +1 на степпер при нескольких вопросах (21:8 «2 + stepper»).
        let multi = ask_state(
            vec![
                question("Q?", false, &["A", "B", "C"], false),
                question("Q1", false, &["A", "B", "C"], false),
            ],
            0,
        );
        assert_eq!(ask_panel_height(&multi, 40, 100), 8);

        // Узкая панель: длинная метка переносится (14:18 += и 14:60 «label + 6»).
        let narrow = ask_state(vec![question("Q", false, &["MMMMMMMMMM"], false)], 0);
        assert_eq!(ask_panel_height(&narrow, 10, 100), 6);

        // Шаг подтверждения: тело = confirm_rows().min(12) = 3.
        let confirm = ask_state(
            vec![
                question("Q?", false, &["A", "B"], true),
                question("Q1", false, &["C", "D"], true),
            ],
            2,
        );
        assert_eq!(ask_panel_height(&confirm, 40, 100), 7);

        // Потолок cap и пол 4 (21 .min(cap).max(4)).
        assert_eq!(ask_panel_height(&custom, 40, 5), 5);
        assert_eq!(ask_panel_height(&custom, 40, 2), 4);
    }

    // ── Часть 2. Мелкие функции ──────────────────────────────────────────────

    /// Маркер строки: стрелка у выбранного, три пробела иначе. Ширина ровно 3 —
    /// иначе номера пунктов разъедутся (183:5 -> ""/"xyzzy").
    #[test]
    fn row_marker_is_arrow_or_three_spaces() {
        assert_eq!(row_marker(true), " › ");
        assert_eq!(row_marker(false), "   ");
        assert_eq!(row_marker(true).chars().count(), 3);
        assert_eq!(row_marker(false).chars().count(), 3);
    }

    /// Подсказка по клавишам — своя на каждый из пяти исходов (302:5 -> ""/"xyzzy").
    #[test]
    fn ask_hint_differs_for_every_mode() {
        let app = ask_app();
        let two = || {
            vec![
                question("Q0", false, &["A", "B"], true),
                question("Q1", false, &["C", "D"], true),
            ]
        };
        let two_multi = || {
            vec![
                question("Q0", true, &["A", "B"], true),
                question("Q1", true, &["C", "D"], true),
            ]
        };

        assert_eq!(
            ask_hint(&ask_state(two(), 2), &app),
            "↑↓ выбрать · Enter правка/отправить · ←/Shift+Tab назад · Esc отмена"
        );
        assert_eq!(
            ask_hint(&ask_state(two_multi(), 0), &app),
            "↑↓ · Space/Enter отметить · Tab дальше · Shift+Tab назад · Esc отмена"
        );
        assert_eq!(
            ask_hint(&ask_state(two(), 0), &app),
            "↑↓ выбрать · Enter/Tab дальше · Shift+Tab назад · Esc отмена"
        );
        assert_eq!(
            ask_hint(
                &ask_state(vec![question("Q", true, &["A", "B"], true)], 0),
                &app
            ),
            "↑↓ · Space отметить · Enter подтвердить · Esc отмена"
        );
        assert_eq!(
            ask_hint(
                &ask_state(vec![question("Q", false, &["A", "B"], true)], 0),
                &app
            ),
            "↑↓ выбрать · Enter подтвердить · Esc отмена"
        );
    }

    /// Степпер: на вопросе жирный «Вопрос i/N», на подтверждении жирное
    /// «Подтверждение». Подсветка идёт от !on_confirm (78:23 delete !).
    #[test]
    fn stepper_bolds_the_current_stage() {
        let app = ask_app();
        let color = app.theme.accent();
        let qs = || {
            vec![
                question("Q0", false, &["A", "B"], true),
                question("Q1", false, &["C", "D"], true),
            ]
        };

        let on_q = stepper_line(&ask_state(qs(), 0), &app, color);
        assert!(line_text(&on_q).starts_with("Вопрос 1/2"));
        let q_span = on_q
            .spans
            .iter()
            .find(|s| s.content.starts_with("Вопрос"))
            .expect("спан «Вопрос i/N»");
        assert!(q_span.style.add_modifier.contains(Modifier::BOLD));
        let c_span = on_q
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "Подтверждение")
            .expect("спан «Подтверждение»");
        assert!(!c_span.style.add_modifier.contains(Modifier::BOLD));

        let on_c = stepper_line(&ask_state(qs(), 2), &app, color);
        assert_eq!(line_text(&on_c), "Вопросы  ·  Подтверждение"); // 77:5 Default → пусто
        let c_span = on_c
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "Подтверждение")
            .expect("спан «Подтверждение»");
        assert!(c_span.style.add_modifier.contains(Modifier::BOLD));
    }

    // ── Часть 3. custom_field_line (здесь паники) ────────────────────────────

    /// ★ Длинный «свой ответ» на узкой панели: показан ТОЛЬКО последний room-хвост,
    /// без underflow-паники (221:48 > и 222:35 -). room = iw - 19 = 10 при iw=29.
    #[test]
    fn custom_field_shows_only_the_tail_of_a_long_answer() {
        let app = ask_app();
        let long = "0123456789".repeat(10); // 100 симв., последние 10 = "0123456789"
        let line = custom_field_line(&custom_answer(&long), &app, app.theme.accent(), 29, true, 4);
        let texts = span_texts(&line);

        assert_eq!(texts.len(), 5); // маркер, номер, метка, хвост, курсор
        assert_eq!(texts[3], "0123456789"); // 217/221/222 сдвинули бы границу или запаниковали
        assert_eq!(line.spans[3].style.fg, Some(Color::White));
        assert_eq!(texts[4], "▌");
    }

    /// Граница хвоста: ровно room показан целиком, room+1 — уже усечён до последних room
    /// символов. Ловит смещение границы (217:39 «used + 1») точным сдвигом на один символ.
    #[test]
    fn custom_field_tail_boundary_at_room_and_room_plus_one() {
        let app = ask_app();
        // iw=29 → room=10.
        let exactly = "0123456789"; // 10 == room → целиком
        let line = custom_field_line(
            &custom_answer(exactly),
            &app,
            app.theme.accent(),
            29,
            true,
            4,
        );
        assert_eq!(span_texts(&line)[3], "0123456789");

        let over = "0123456789A"; // 11 == room+1 → последние 10 символов
        let line = custom_field_line(&custom_answer(over), &app, app.theme.accent(), 29, true, 4);
        assert_eq!(span_texts(&line)[3], "123456789A");
    }

    /// Выбран + короткий непустой ответ → показан ЦЕЛИКОМ, затем курсор
    /// (219:12 delete ! и 219:46 > -> ==/< выбросили бы текст).
    #[test]
    fn custom_field_shows_a_short_answer_in_full() {
        let app = ask_app();
        let line = custom_field_line(&custom_answer("hi"), &app, app.theme.accent(), 29, true, 4);
        let texts = span_texts(&line);
        assert_eq!(texts.len(), 5);
        assert_eq!(texts[3], "hi");
        assert_eq!(line.spans[3].style.fg, Some(Color::White));
        assert_eq!(texts[4], "▌");
    }

    /// Выбран + пусто → только курсор, без плейсхолдера и без пустого спана
    /// (219:38 && -> || добавил бы лишний пустой спан).
    #[test]
    fn custom_field_selected_and_empty_is_just_a_cursor() {
        let app = ask_app();
        let line = custom_field_line(&custom_answer(""), &app, app.theme.accent(), 29, true, 4);
        let texts = span_texts(&line);
        assert_eq!(texts.len(), 4);
        assert_eq!(texts[3], "▌");
    }

    /// room == 0 (iw=19): текст не показываем вовсе (219:46 > -> >= влез бы в пустой срез).
    #[test]
    fn custom_field_with_no_room_skips_the_text() {
        let app = ask_app();
        let line = custom_field_line(&custom_answer("hi"), &app, app.theme.accent(), 19, true, 4);
        let texts = span_texts(&line);
        assert_eq!(texts.len(), 4);
        assert_eq!(texts[3], "▌");
    }

    /// Не выбран: пусто → курсивный плейсхолдер; непусто → серый текст
    /// (198:5 Default::default дал бы пустую строку без спанов).
    #[test]
    fn custom_field_unselected_shows_placeholder_or_muted_text() {
        let app = ask_app();
        let empty = custom_field_line(&custom_answer(""), &app, app.theme.accent(), 29, false, 4);
        assert_eq!(span_texts(&empty).last().unwrap(), "впишите свой вариант");

        let filled =
            custom_field_line(&custom_answer("hi"), &app, app.theme.accent(), 29, false, 4);
        assert_eq!(span_texts(&filled).last().unwrap(), "hi");
        assert_eq!(filled.spans.last().unwrap().style.fg, Some(MUTED));
    }

    // ── Часть 4. Рисование строк ─────────────────────────────────────────────

    /// Вопрос с вариантами и «Свой ответ»: маркер и жирный стиль у выбранного, номера,
    /// custom-строка с правильным номером. Закрывает 110/124/126/127/128/174.
    #[test]
    fn question_rows_render_options_marker_and_custom_line() {
        let app = ask_app();
        let mut st = ask_state(vec![question("Q?", false, &["Codex", "Claude"], true)], 0);
        st.answers[0].cursor = 0;

        let mut lines = Vec::new();
        draw_question_rows(&st, &app, app.theme.accent(), 40, 10, &mut lines);

        assert_eq!(lines.len(), 4); // вопрос + 2 варианта + «Свой ответ»
        assert!(has(&lines, "Codex")); // 110 ->(), 126 *, 128 >
        assert!(has(&lines, "Claude")); // 124 + -> -
        assert!(has(&lines, "Свой ответ")); // 124 + -> *
        assert!(line_text(find(&lines, "Codex")).contains("›")); // 127 == -> !=
        assert!(!line_text(find(&lines, "Claude")).contains("›"));
        assert!(line_text(find(&lines, "Свой ответ")).contains("3.")); // 174 options+1

        // Выбранный вариант — белый жирный, невыбранный — нет.
        let codex = find(&lines, "Codex");
        let codex_label = codex
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "Codex")
            .expect("спан варианта Codex");
        assert_eq!(codex_label.style.fg, Some(Color::White));
        assert!(codex_label.style.add_modifier.contains(Modifier::BOLD));
        let claude = find(&lines, "Claude");
        let claude_label = claude
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "Claude")
            .expect("спан варианта Claude");
        assert_ne!(claude_label.style.fg, Some(Color::White));
        assert!(!claude_label.style.add_modifier.contains(Modifier::BOLD));
    }

    /// Курсор на строке «Свой ответ» (cursor == options.len()): сама `draw_question_rows`
    /// уходит в ветку custom (128:16 `<`), рисует выбранное поле — маркер, введённый текст
    /// и курсор, — а варианты выше остаются невыбранными.
    #[test]
    fn question_rows_render_selected_custom_row() {
        let app = ask_app();
        let mut st = ask_state(vec![question("Q?", false, &["A", "B"], true)], 0);
        st.answers[0].cursor = 2; // == options.len() → строка «Свой ответ»
        st.answers[0].custom = "мой".to_string();

        let mut lines = Vec::new();
        draw_question_rows(&st, &app, app.theme.accent(), 40, 10, &mut lines);

        let custom = find(&lines, "Свой ответ");
        assert!(line_text(custom).starts_with(" › ")); // выбрана именно custom-строка
        assert!(line_text(custom).contains("мой")); // введённый текст показан
        assert!(line_text(custom).ends_with("▌")); // курсор редактирования
                                                   // Варианты выше — не выбраны (маркер-стрелка только у custom).
        assert!(line_text(find(&lines, "A")).starts_with("   "));
        assert!(line_text(find(&lines, "B")).starts_with("   "));
    }

    /// Множественный вопрос рисует чекбоксы — «[x]» у отмеченного, «[ ]» у прочих.
    #[test]
    fn multi_question_shows_checkboxes() {
        let app = ask_app();
        let mut st = ask_state(vec![question("Что?", true, &["Тесты", "Доки"], false)], 0);
        st.answers[0].checked = vec![true, false];

        let mut lines = Vec::new();
        draw_question_rows(&st, &app, app.theme.accent(), 40, 10, &mut lines);
        assert!(line_text(find(&lines, "Тесты")).contains("[x]"));
        assert!(line_text(find(&lines, "Доки")).contains("[ ]"));
    }

    /// Скролл: вариантов больше, чем влезает; видно окно вокруг курсора, верхние ушли
    /// (122:37 capacity меняет границы окна).
    #[test]
    fn question_rows_scroll_around_the_cursor() {
        let app = ask_app();
        let labels: Vec<String> = (0..6).map(|i| format!("opt{i}")).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let mut st = ask_state(vec![question("Q?", false, &refs, false)], 0);
        st.answers[0].cursor = 5;

        let mut lines = Vec::new();
        draw_question_rows(&st, &app, app.theme.accent(), 40, 5, &mut lines);

        assert!(has(&lines, "opt5")); // курсор в окне
        assert!(!has(&lines, "opt0"));
        assert!(!has(&lines, "opt1")); // 122 + -> -
        assert!(!has(&lines, "opt2")); // 122 + -> *
    }

    /// Аннотация варианта показывается, когда влезает, и режется по room
    /// (157:51 «used + 3» и 158:25 room > 4).
    #[test]
    fn option_note_is_shown_and_truncated_by_room() {
        let app = ask_app();
        let q = AskQuestion {
            question: "Q".into(),
            multi: false,
            options: vec![AskOption {
                label: "AB".into(),
                note: Some("abcdefghijklmno".into()),
            }],
            allow_custom: false,
        };
        let st = ask_state(vec![q], 0);

        // iw=21 → room=10: note виден и усечён до 10 симв. (9 + …).
        let mut wide = Vec::new();
        draw_question_rows(&st, &app, app.theme.accent(), 21, 10, &mut wide);
        assert!(line_text(find(&wide, "AB")).contains(" — abcdefghi…"));
        assert!(!line_text(find(&wide, "AB")).contains("abcdefghij")); // не полностью

        // iw=15 → room=4: note скрыт (room > 4 ложно; 158 >= показал бы).
        let mut narrow = Vec::new();
        draw_question_rows(&st, &app, app.theme.accent(), 15, 10, &mut narrow);
        assert!(!line_text(find(&narrow, "AB")).contains(" — "));
    }

    /// Подтверждение: строки «N. вопрос: ответ», «Отправить», маркер и стиль на курсоре,
    /// вопрос усечён по iw/2. Закрывает 251/255/256/258/266.
    #[test]
    fn confirm_rows_render_questions_send_and_truncate() {
        let app = ask_app();
        let long_q = "Abcdefghijklmnopqrstuvwxyz1234"; // 30 симв.
        let mut st = ask_state(
            vec![
                question(long_q, false, &["X", "Y"], true),
                question("Q1", false, &["Z", "W"], true),
            ],
            2,
        );
        st.confirm_cursor = 0;

        let mut lines = Vec::new();
        draw_confirm_rows(&st, &app, app.theme.accent(), 40, 10, &mut lines);

        assert_eq!(lines.len(), 3);
        assert!(line_text(&lines[0]).starts_with(" › 1.")); // 256 == -> !=
        assert_eq!(lines[0].spans[0].style.fg, Some(Color::White)); // выбранная строка ярче
        assert!(line_text(&lines[0]).contains("Abcdefghijklmnopqrs…")); // 266 iw/2
        assert!(!line_text(&lines[0]).contains("Abcdefghijklmnopqrst"));
        assert!(has(&lines, "Q1")); // 258 < -> > дал бы только «Отправить»
        assert!(has(&lines, "Отправить ответы")); // 251 ->(), 255 +
    }

    /// Ответ в строке подтверждения берётся из `chosen(i)`, а не теряется/заменяется:
    /// множественный вопрос с отметкой + свой ответ, и одиночный с курсором — оба
    /// показывают именно выбранное содержимое.
    #[test]
    fn confirm_rows_show_chosen_answers() {
        let app = ask_app();
        let mut st = ask_state(
            vec![
                question("Что?", true, &["Тесты", "Доки"], true),
                question("Кто?", false, &["Codex", "Claude"], true),
            ],
            2,
        );
        st.answers[0].checked = vec![true, false];
        st.answers[0].custom = "  и рендер  ".to_string();
        st.answers[1].cursor = 1; // «Claude»

        let mut lines = Vec::new();
        draw_confirm_rows(&st, &app, app.theme.accent(), 60, 10, &mut lines);

        // Q0: отмеченная подпись + свой ответ (trim) через chosen(0).join(", ").
        let q0 = find(&lines, "Что?");
        assert!(line_text(q0).contains("Тесты"));
        assert!(line_text(q0).contains("и рендер"));
        assert!(!line_text(q0).contains("Доки")); // неотмеченное не попадает
                                                  // Q1: одиночный выбор через chosen(1) — именно «Claude», не «Codex».
        let q1 = find(&lines, "Кто?");
        assert!(line_text(q1).contains("Claude"));
        assert!(!line_text(q1).contains("Codex"));
    }

    /// Маркер выбора на строке «Отправить», когда курсор на ней (258 разделяет
    /// вопросы и «Отправить»); строка вопроса при этом не выбрана.
    #[test]
    fn confirm_send_row_is_marked_when_selected() {
        let app = ask_app();
        let mut st = ask_state(
            vec![
                question("Q0", false, &["A", "B"], true),
                question("Q1", false, &["C", "D"], true),
            ],
            2,
        );
        st.confirm_cursor = 2; // строка «Отправить»

        let mut lines = Vec::new();
        draw_confirm_rows(&st, &app, app.theme.accent(), 40, 10, &mut lines);

        assert!(line_text(find(&lines, "Отправить ответы")).starts_with(" › "));
        assert!(line_text(find(&lines, "Q0")).starts_with("   ")); // вопрос не выбран
    }

    /// Скролл подтверждения: окно едет вокруг курсора, верхние вопросы уходят
    /// (252:66 capacity).
    #[test]
    fn confirm_rows_scroll_around_the_cursor() {
        let app = ask_app();
        let qs: Vec<AskQuestion> = (0..5)
            .map(|i| question(&format!("cq{i}"), false, &["a", "b"], true))
            .collect();
        let mut st = ask_state(qs, 5);
        st.confirm_cursor = 5; // строка «Отправить»

        let mut lines = Vec::new();
        draw_confirm_rows(&st, &app, app.theme.accent(), 40, 4, &mut lines);

        assert!(has(&lines, "Отправить ответы"));
        assert!(has(&lines, "cq3"));
        assert!(!has(&lines, "cq2")); // 252 + -> * протолкнул бы cq2 в окно
    }

    /// Панель целиком: рамка с «Выбор», текст вопроса и варианты. Закрывает
    /// 33:5 ->(), 36:20 (пустой экран) и 50:21 (рамка без тела).
    #[test]
    fn panel_draws_frame_question_and_options() {
        let mut app = ask_app();
        app.ask = Some(ask_state(
            vec![question("Провайдер?", false, &["Codex", "Claude"], true)],
            0,
        ));

        let text = buffer_rows(&ask_screen(&app, 40, 20)).join("\n");
        assert!(text.contains("Выбор")); // 33:5, 36:20
        assert!(text.contains("Провайдер")); // 50:21
        assert!(text.contains("Codex"));
    }

    /// Схлопнутая панель не паникует: нулевая высота area и нулевая inner.
    #[test]
    fn panel_survives_tiny_areas() {
        let mut app = ask_app();
        app.ask = Some(ask_state(vec![question("Q?", false, &["A", "B"], true)], 0));

        let mut terminal = Terminal::new(TestBackend::new(40, 6)).expect("оффскрин-терминал");
        terminal
            .draw(|f| {
                draw_ask_panel(f, Rect::new(0, 0, 40, 0), &app); // area.height == 0
                draw_ask_panel(f, Rect::new(0, 0, 40, 2), &app); // inner.height == 0
            })
            .expect("отрисовка не паникует");
    }
}
