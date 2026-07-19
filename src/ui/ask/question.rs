use super::*;

pub(crate) fn draw_question_rows(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::ask::testkit::*;

    fn custom_answer(text: &str) -> AnswerState {
        AnswerState {
            cursor: 0,
            checked: Vec::new(),
            custom: text.to_string(),
        }
    }

    fn span_texts(line: &Line<'_>) -> Vec<String> {
        line.spans.iter().map(|s| s.content.to_string()).collect()
    }

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
}
