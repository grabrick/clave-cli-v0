use super::*;

/// Ответ на один вопрос: позиция курсора, отметки (multi) и текст «своего ответа».
/// Курсор ходит по `0..=options.len()`; последний индекс — строка «Свой ответ».
pub(crate) struct AnswerState {
    pub(crate) cursor: usize,
    pub(crate) checked: Vec<bool>,
    pub(crate) custom: String,
}

impl AnswerState {
    fn new(options: usize) -> Self {
        Self {
            cursor: 0,
            checked: vec![false; options],
            custom: String::new(),
        }
    }
}

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
}

impl AskState {
    fn new(prompt: AskPrompt) -> Self {
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
    pub(crate) fn ask_active(&self) -> bool {
        self.ask.is_some()
    }

    /// Открывает селектор из отложенного запроса (после того как «допечаталась» проза).
    pub(crate) fn open_pending_ask(&mut self) {
        if let Some(prompt) = self.ask_prompt_pending.take() {
            self.ask = Some(AskState::new(prompt));
            self.status = self.lang.choose("выбор", "choose").to_string();
        }
    }

    pub(crate) fn reset_ask(&mut self) {
        self.ask = None;
        self.ask_prompt_pending = None;
        self.ask_intent = None;
    }

    /// Ответ на ЛОКАЛЬНЫЙ вопрос (например «включить зрение?» перед `/dev`) обрабатываем
    /// сами. Обычный путь `ask_submit` отправляет выбор модели через `start_chat` — здесь
    /// это было бы бессмысленно: вопрос задало само приложение, ему и решать.
    fn submit_ask_intent(&mut self) {
        let Some(state) = &self.ask else {
            return;
        };
        let question = &state.prompt.questions[0];
        let answer = &state.answers[0];
        if answer.cursor >= question.options.len() {
            return; // строка «свой ответ» для локальных вопросов не поддерживается
        }
        let choice = answer.cursor;
        self.ask = None;
        match self.ask_intent.take() {
            // Вариант 1 (индекс 1) — «Да, зрение».
            Some(AskIntent::DevVision { task }) => self.start_dev(task, choice == 1),
            None => {}
        }
    }

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

    /// Enter в визарде (несколько вопросов): на множественном варианте — отметить
    /// (как Space, переход дальше — только Tab); на одиночном/строке «свой ответ» —
    /// дальше; на подтверждении — отправить или вернуться к правке вопроса.
    /// Один вопрос — отправляем сразу (свой ответ или выбор).
    pub(crate) fn ask_submit(&mut self) {
        // Локальный вопрос приложения (напр. зрение для /dev) — отвечаем сами, модели не шлём.
        if self.ask_intent.is_some() {
            self.submit_ask_intent();
            return;
        }

        let Some(state) = &self.ask else {
            return;
        };

        if state.multi_question() {
            let on_confirm = state.on_confirm();
            let on_send = state.on_send_row();
            let target = state.confirm_cursor;
            let toggle_here = state.question().is_some_and(|q| q.multi) && !state.on_custom_row();
            if on_confirm {
                if on_send {
                    self.ask_send_all();
                } else if let Some(state) = &mut self.ask {
                    state.step = target; // вернуться к правке выбранного вопроса
                }
            } else if toggle_here {
                self.ask_toggle(); // множественный: Enter отмечает вариант, не прыгает
            } else {
                self.ask_next(); // одиночный или «свой ответ» → следующий шаг
            }
            return;
        }

        // ── одиночный вопрос: формируем сообщение и отправляем ──
        let q = &state.prompt.questions[0];
        let a = &state.answers[0];
        let message = if a.cursor == q.options.len() {
            let text = a.custom.trim().to_string();
            if text.is_empty() {
                return; // поле «своего ответа» пустое — ждём ввода (Esc — выйти)
            }
            text
        } else {
            let labels: Vec<String> = if q.multi {
                q.options
                    .iter()
                    .zip(&a.checked)
                    .filter(|(_, &checked)| checked)
                    .map(|(opt, _)| opt.label.clone())
                    .collect()
            } else {
                vec![q.options[a.cursor].label.clone()]
            };
            if labels.is_empty() {
                return; // множественный без отметок — подтверждать нечего
            }
            let joined = labels
                .iter()
                .map(|label| format!("«{label}»"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} {}", self.lang.choose("Выбрано:", "Selected:"), joined)
        };
        self.ask = None;
        self.start_chat(message);
    }

    /// Собирает ответы на все вопросы в одно сообщение и отправляет модели.
    fn ask_send_all(&mut self) {
        let Some(state) = &self.ask else {
            return;
        };
        let mut lines = Vec::new();
        for (i, q) in state.prompt.questions.iter().enumerate() {
            let chosen = state.chosen(i);
            let answer = if chosen.is_empty() {
                self.lang.choose("(пропущено)", "(skipped)").to_string()
            } else {
                chosen.join(", ")
            };
            lines.push(format!("{}. {}: {}", i + 1, q.question, answer));
        }
        let header = self.lang.choose("Ответы:", "Answers:");
        let message = format!("{header}\n{}", lines.join("\n"));
        self.ask = None;
        self.start_chat(message);
    }

    /// Esc: закрыть селектор и дать ответить текстом (вопрос остаётся в ленте).
    /// Для локального вопроса приложения текстом отвечать некому — значит это отмена
    /// самого действия (напр. `/dev` не стартует вовсе).
    pub(crate) fn ask_cancel(&mut self) {
        if self.ask.take().is_some() {
            if self.ask_intent.take().is_some() {
                self.push_system(self.lang.choose("⏹ /dev отменён.", "⏹ /dev cancelled."));
                self.status = self.lang.choose("отменено", "cancelled").to_string();
                return;
            }
            self.status = self.lang.choose("закрыто", "closed").to_string();
        }
    }
}
