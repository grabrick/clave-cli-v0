use super::*;

pub(crate) fn draw_confirm_rows(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::ask::testkit::*;

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
}
