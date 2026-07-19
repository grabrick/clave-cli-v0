use super::*;

/// Активный inline-селектор — визард на один или несколько вопросов.
///
/// `step` ∈ `0..questions.len()` — отвечаем на вопрос; `== questions.len()` — шаг
/// подтверждения (бывает только при нескольких вопросах). На подтверждении
/// `confirm_cursor` ходит по строкам: вопрос_i … затем «Отправить».
pub(crate) struct AskState {
    pub(crate) prompt: AskPrompt,
    pub(crate) answers: Vec<AnswerState>,
    pub(crate) step: usize,
    pub(crate) confirm_cursor: usize,
    /// Выбор питает ПАУЗУ ТАНДЕМА (заблокированный воркер), а не новый чат-прогон: на
    /// подтверждении шлём результат в `tandem_input_tx`, а не в `start_chat`.
    pub(crate) feeds_tandem: bool,
}

impl AskState {
    pub(crate) fn new(prompt: AskPrompt) -> Self {
        let answers = prompt
            .questions
            .iter()
            .map(|q| AnswerState::new(q.options.len()))
            .collect();
        Self {
            prompt,
            answers,
            step: 0,
            confirm_cursor: 0,
            feeds_tandem: false,
        }
    }

    pub(crate) fn multi_question(&self) -> bool {
        self.prompt.questions.len() > 1
    }

    /// Сейчас открыт шаг подтверждения?
    pub(crate) fn on_confirm(&self) -> bool {
        self.step >= self.prompt.questions.len()
    }

    /// Текущий вопрос (None на шаге подтверждения).
    pub(crate) fn question(&self) -> Option<&AskQuestion> {
        self.prompt.questions.get(self.step)
    }

    pub(crate) fn current_answer(&self) -> Option<&AnswerState> {
        self.answers.get(self.step)
    }

    /// Курсор на строке «Свой ответ» текущего вопроса?
    pub(crate) fn on_custom_row(&self) -> bool {
        match (self.question(), self.current_answer()) {
            (Some(q), Some(a)) => a.cursor == q.options.len(),
            _ => false,
        }
    }

    /// Строк на шаге подтверждения: вопросы + «Отправить».
    pub(crate) fn confirm_rows(&self) -> usize {
        self.prompt.questions.len() + 1
    }

    pub(crate) fn on_send_row(&self) -> bool {
        self.on_confirm() && self.confirm_cursor == self.prompt.questions.len()
    }

    /// Выбранные подписи для вопроса `i` (для показа на подтверждении и для отправки).
    pub(crate) fn chosen(&self, i: usize) -> Vec<String> {
        let (Some(q), Some(a)) = (self.prompt.questions.get(i), self.answers.get(i)) else {
            return Vec::new();
        };
        let custom = a.custom.trim();
        if q.multi {
            let mut out: Vec<String> = q
                .options
                .iter()
                .zip(&a.checked)
                .filter(|(_, &checked)| checked)
                .map(|(opt, _)| opt.label.clone())
                .collect();
            if !custom.is_empty() {
                out.push(custom.to_string());
            }
            out
        } else if a.cursor < q.options.len() {
            vec![q.options[a.cursor].label.clone()]
        } else if !custom.is_empty() {
            vec![custom.to_string()]
        } else {
            Vec::new()
        }
    }
}

impl App {
    /// ↑↓: двигает курсор в текущем списке (варианты вопроса или строки подтверждения).
    pub(crate) fn ask_move(&mut self, delta: isize) {
        let Some(state) = &mut self.ask else {
            return;
        };
        if state.on_confirm() {
            let rows = state.confirm_rows() as isize;
            state.confirm_cursor =
                (state.confirm_cursor as isize + delta).rem_euclid(rows) as usize;
        } else {
            let step = state.step;
            let question = &state.prompt.questions[step];
            // Курсор ходит по вариантам и, если он есть, по строке «Свой ответ».
            let rows = (question.options.len() + usize::from(question.allow_custom)) as isize;
            if let Some(answer) = state.answers.get_mut(step) {
                answer.cursor = (answer.cursor as isize + delta).rem_euclid(rows) as usize;
            }
        }
    }

    /// Tab/→: следующий вопрос (или шаг подтверждения). Для одиночного — нет хода.
    pub(crate) fn ask_next(&mut self) {
        if let Some(state) = &mut self.ask {
            if state.multi_question() && state.step < state.prompt.questions.len() {
                state.step += 1;
                state.confirm_cursor = 0;
            }
        }
    }

    /// Shift+Tab/←: предыдущий вопрос (с подтверждения — к последнему вопросу).
    pub(crate) fn ask_prev(&mut self) {
        if let Some(state) = &mut self.ask {
            if state.multi_question() && state.step > 0 {
                state.step -= 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::ask::testkit::*;

    /// Подписи выбранного: множественный отдаёт отметки и свой ответ, одиночный — один
    /// вариант либо свой текст; пустой свой ответ не отдаёт ничего.
    #[test]
    fn chosen_labels_cover_multi_single_and_custom_rows() {
        let mut app = app_for_ask();
        open_ask(
            &mut app,
            vec![
                question("Что затронуть?", true, &["Тесты", "Доки"], true),
                question("Провайдер?", false, &["Codex", "Claude"], true),
            ],
        );
        {
            let ask = state(&mut app);
            ask.answers[0].checked[0] = true;
            ask.answers[0].custom = "  и рендер  ".to_string();
            ask.answers[1].cursor = 0;
        }
        {
            let ask = app.ask.as_ref().expect("селектор открыт");
            assert_eq!(ask.chosen(0), vec!["Тесты", "и рендер"]);
            assert_eq!(ask.chosen(1), vec!["Codex"]);
        }

        // Одиночный: курсор на строке «Свой ответ» — идёт свой текст.
        {
            let ask = state(&mut app);
            ask.answers[1].cursor = 2;
            ask.answers[1].custom = "оба".to_string();
        }
        assert_eq!(
            app.ask.as_ref().expect("селектор").chosen(1),
            vec!["оба".to_string()]
        );

        // ...а пустой свой ответ — это ничего, а не пустая строка.
        state(&mut app).answers[1].custom = "   ".to_string();
        assert!(app.ask.as_ref().expect("селектор").chosen(1).is_empty());
    }

    /// На подтверждении курсор ходит по строкам вопросов и «Отправить», заворачиваясь.
    #[test]
    fn confirm_cursor_wraps_over_questions_and_the_send_row() {
        let mut app = app_for_ask();
        open_ask(
            &mut app,
            vec![
                question("Первый?", false, &["A"], true),
                question("Второй?", false, &["B"], true),
            ],
        );
        state(&mut app).step = 2;

        // Три строки: вопрос 0, вопрос 1, «Отправить».
        assert_eq!(app.ask.as_ref().expect("селектор").confirm_rows(), 3);

        app.ask_move(-1);
        assert_eq!(
            state(&mut app).confirm_cursor,
            2,
            "вверх с нуля — на «Отправить»"
        );

        state(&mut app).confirm_cursor = 1;
        app.ask_move(1);
        assert_eq!(state(&mut app).confirm_cursor, 2, "вниз — на «Отправить»");
    }

    /// Шаги визарда: вперёд не дальше подтверждения, назад не дальше нуля,
    /// а одиночный вопрос шагов не имеет вовсе.
    #[test]
    fn wizard_steps_stay_inside_their_bounds() {
        let mut app = app_for_ask();
        open_ask(
            &mut app,
            vec![
                question("Первый?", false, &["A"], true),
                question("Второй?", false, &["B"], true),
            ],
        );
        state(&mut app).step = 2; // подтверждение — дальше некуда
        app.ask_next();
        assert_eq!(state(&mut app).step, 2);

        state(&mut app).step = 0;
        app.ask_prev();
        assert_eq!(state(&mut app).step, 0, "с нулевого шага назад не уходим");
        app.ask_next();
        assert_eq!(state(&mut app).step, 1);

        // Одиночный вопрос: шагов нет ни вперёд, ни назад.
        let mut app = app_for_ask();
        open_ask(&mut app, vec![question("Один?", false, &["A"], true)]);
        app.ask_next();
        assert_eq!(state(&mut app).step, 0, "одиночный вопрос не шагает вперёд");
        state(&mut app).step = 1;
        app.ask_prev();
        assert_eq!(state(&mut app).step, 1, "одиночный вопрос не шагает назад");
    }
}
