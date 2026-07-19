use super::*;

/// Ответ на один вопрос: позиция курсора, отметки (multi) и текст «своего ответа».
/// Курсор ходит по `0..=options.len()`; последний индекс — строка «Свой ответ».
pub(crate) struct AnswerState {
    pub(crate) cursor: usize,
    pub(crate) checked: Vec<bool>,
    pub(crate) custom: String,
}

impl AnswerState {
    pub(crate) fn new(options: usize) -> Self {
        Self {
            cursor: 0,
            checked: vec![false; options],
            custom: String::new(),
        }
    }
}

impl App {
    /// Space: отметить/снять вариант (только для множественного выбора).
    pub(crate) fn ask_toggle(&mut self) {
        let Some(state) = &mut self.ask else {
            return;
        };
        let step = state.step;
        if step >= state.prompt.questions.len() {
            return; // подтверждение — отмечать нечего
        }
        let (multi, opts) = {
            let q = &state.prompt.questions[step];
            (q.multi, q.options.len())
        };
        if let Some(answer) = state.answers.get_mut(step) {
            if multi && answer.cursor < opts {
                let i = answer.cursor;
                answer.checked[i] = !answer.checked[i];
            }
        }
    }

    pub(crate) fn ask_on_custom_row(&self) -> bool {
        self.ask.as_ref().is_some_and(AskState::on_custom_row)
    }

    pub(crate) fn ask_custom_push(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        let Some(state) = &mut self.ask else {
            return;
        };
        if state.on_custom_row() {
            let step = state.step;
            if let Some(answer) = state.answers.get_mut(step) {
                answer.custom.push(ch);
            }
        }
    }

    pub(crate) fn ask_custom_backspace(&mut self) {
        let Some(state) = &mut self.ask else {
            return;
        };
        if state.on_custom_row() {
            let step = state.step;
            if let Some(answer) = state.answers.get_mut(step) {
                answer.custom.pop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::ask::testkit::*;

    /// Space отмечает только вариант множественного вопроса.
    #[test]
    fn toggle_only_marks_options_of_a_multi_question() {
        let mut app = app_for_ask();
        open_ask(
            &mut app,
            vec![question("Что затронуть?", true, &["Тесты", "Доки"], true)],
        );

        app.ask_toggle();
        assert!(state(&mut app).answers[0].checked[0], "вариант отмечен");
        app.ask_toggle();
        assert!(!state(&mut app).answers[0].checked[0], "и снят повторно");

        // Строка «свой ответ» отметок не имеет — отмечать нечего.
        state(&mut app).answers[0].cursor = 2;
        app.ask_toggle();
        assert_eq!(state(&mut app).answers[0].checked, vec![false, false]);

        // Одиночный вопрос не отмечается вовсе.
        let mut app = app_for_ask();
        open_ask(
            &mut app,
            vec![question("Провайдер?", false, &["Codex"], true)],
        );
        app.ask_toggle();
        assert_eq!(state(&mut app).answers[0].checked, vec![false]);
    }
}
