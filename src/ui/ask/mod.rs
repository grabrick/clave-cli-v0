use super::*;

mod confirm;
mod height;
mod question;
pub(crate) use confirm::*;
pub(crate) use height::*;
pub(crate) use question::*;

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

/// Отступ строки-пункта: стрелка на выбранном, пробелы иначе. Один пробел слева
/// для воздуха, чтобы пункты были чуть отбиты от рамки и текста вопроса.
fn row_marker(selected: bool) -> &'static str {
    if selected {
        " › "
    } else {
        "   "
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
pub(crate) mod testkit {
    use super::*;
    use ratatui::{backend::TestBackend, buffer::Buffer};

    // ── общие хелперы ────────────────────────────────────────────────────────

    /// App на своих временных путях. `onboarding_done: true` — иначе поднимется
    /// auth-probe, и тест начнёт зависеть от установленных claude/codex (на CI их нет).
    pub(crate) fn ask_app() -> App {
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

    pub(crate) fn opt(label: &str) -> AskOption {
        AskOption {
            label: label.to_string(),
            note: None,
        }
    }

    pub(crate) fn question(
        text: &str,
        multi: bool,
        labels: &[&str],
        allow_custom: bool,
    ) -> AskQuestion {
        AskQuestion {
            question: text.to_string(),
            multi,
            options: labels.iter().map(|l| opt(l)).collect(),
            allow_custom,
        }
    }

    pub(crate) fn answer(options: usize) -> AnswerState {
        AnswerState {
            cursor: 0,
            checked: vec![false; options],
            custom: String::new(),
        }
    }

    /// Состояние на шаге `step`. Курсоры/отметки/custom правим у полей после сборки.
    pub(crate) fn ask_state(questions: Vec<AskQuestion>, step: usize) -> AskState {
        let answers = questions.iter().map(|q| answer(q.options.len())).collect();
        AskState {
            prompt: AskPrompt { questions },
            answers,
            step,
            confirm_cursor: 0,
            feeds_tandem: false,
        }
    }

    pub(crate) fn line_text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    pub(crate) fn has(lines: &[Line<'static>], needle: &str) -> bool {
        lines.iter().any(|l| line_text(l).contains(needle))
    }

    pub(crate) fn find<'a>(lines: &'a [Line<'static>], needle: &str) -> &'a Line<'static> {
        lines
            .iter()
            .find(|l| line_text(l).contains(needle))
            .unwrap_or_else(|| panic!("нет строки с «{needle}»"))
    }

    pub(crate) fn ask_screen(app: &App, w: u16, h: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("оффскрин-терминал");
        terminal
            .draw(|f| draw_ask_panel(f, f.area(), app))
            .expect("отрисовка панели");
        terminal.backend().buffer().clone()
    }

    pub(crate) fn buffer_rows(buffer: &Buffer) -> Vec<String> {
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
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;
    use ratatui::backend::TestBackend;

    // ── Часть 1. Чистая арифметика ───────────────────────────────────────────

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
