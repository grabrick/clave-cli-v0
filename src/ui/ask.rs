use super::*;

/// Высота панели селектора: рамка + (степпер) + контент + подсказка.
pub(crate) fn ask_panel_height(state: &AskState, width: u16, cap: u16) -> u16 {
    let stepper = u16::from(state.multi_question());
    let iw = (width as usize).saturating_sub(2).max(8); // минус рамка
    let body = if state.on_confirm() {
        (state.confirm_rows() as u16).min(12)
    } else if let Some(question) = state.question() {
        // Высота с учётом переноса: строка(и) вопроса + варианты + «Свой ответ».
        let mut rows = wrapped_rows(display_width(&question.question), iw);
        for opt in &question.options {
            // ~6 символов на маркер/номер/чекбокс перед текстом варианта.
            rows += wrapped_rows(display_width(&opt.label) + 6, iw);
        }
        rows += 1;
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
    let total = question.options.len() + 1; // + «Свой ответ»
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
