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
    pub(crate) fn ask_cancel(&mut self) {
        if self.ask.take().is_some() {
            self.status = self.lang.choose("закрыто", "closed").to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Каталог уникален на процесс И на вызов: параллельные прогоны иначе затирают
    /// файлы друг друга, и мутационный гейт получает случайные падения.
    fn temp_ask_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = std::env::temp_dir().join(format!(
            "clave-ask-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    /// App на своих временных путях. `running = true` — обязателен всюду, где ответ уходит
    /// модели: тогда сообщение встаёт в очередь и живой CLI провайдера не поднимается.
    fn app_for_ask() -> App {
        let dir = temp_ask_dir();
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
        app.onboarding = None;
        app.overlay = Overlay::None;
        app.transcript.clear();
        app
    }

    fn question(text: &str, multi: bool, options: &[&str], allow_custom: bool) -> AskQuestion {
        AskQuestion {
            question: text.to_string(),
            multi,
            options: options
                .iter()
                .map(|label| AskOption {
                    label: (*label).to_string(),
                    note: None,
                })
                .collect(),
            allow_custom,
        }
    }

    fn open_ask(app: &mut App, questions: Vec<AskQuestion>) {
        app.ask = Some(AskState::new(AskPrompt { questions }));
    }

    fn state(app: &mut App) -> &mut AskState {
        app.ask.as_mut().expect("селектор открыт")
    }

    /// Главный контракт селектора: то, что человек выбрал, ОБЯЗАНО уйти модели. Пустышка
    /// вместо ask_send_all = выбрал, нажал Enter — и ничего не ушло.
    #[test]
    fn confirmed_answers_are_sent_to_the_model() {
        let mut app = app_for_ask();
        app.running = true; // очередь вместо живого CLI
        open_ask(
            &mut app,
            vec![
                question("Какой провайдер?", false, &["Codex", "Claude"], true),
                question("Что затронуть?", true, &["Тесты", "Доки"], true),
            ],
        );
        {
            let state = state(&mut app);
            state.answers[0].cursor = 1; // «Claude»
            state.step = 2; // шаг подтверждения
            state.confirm_cursor = 2; // строка «Отправить»
        }

        app.ask_submit();

        assert!(!app.ask_active(), "селектор закрывается после отправки");
        assert_eq!(
            app.pending_messages.iter().cloned().collect::<Vec<_>>(),
            vec![
                "Ответы:\n1. Какой провайдер?: Claude\n2. Что затронуть?: (пропущено)".to_string()
            ]
        );
    }

    /// Enter на строке ВОПРОСА (а не «Отправить») возвращает к правке, а не шлёт.
    #[test]
    fn enter_on_a_question_row_returns_to_editing_instead_of_sending() {
        let mut app = app_for_ask();
        app.running = true;
        open_ask(
            &mut app,
            vec![
                question("Первый?", false, &["A", "B"], true),
                question("Второй?", false, &["C", "D"], true),
            ],
        );
        {
            let state = state(&mut app);
            state.step = 2;
            state.confirm_cursor = 0; // строка первого вопроса
        }

        app.ask_submit();

        assert!(app.ask_active(), "селектор остаётся открыт");
        assert_eq!(
            state(&mut app).step,
            0,
            "вернулись к правке первого вопроса"
        );
        assert!(
            app.pending_messages.is_empty(),
            "с шага правки модели ничего не уходит"
        );
    }

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

    #[test]
    fn reset_ask_closes_the_selector_and_drops_the_pending_prompt() {
        let mut app = app_for_ask();
        open_ask(
            &mut app,
            vec![question("Зрение?", false, &["Нет", "Да"], false)],
        );
        app.ask_prompt_pending = Some(AskPrompt {
            questions: vec![question("Ещё?", false, &["A"], false)],
        });
        app.reset_ask();

        assert!(!app.ask_active(), "селектор закрыт");
        assert!(app.ask_prompt_pending.is_none());
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

    /// Enter внутри визарда: на варианте множественного — отмечает (шаг НЕ двигает),
    /// на «своём ответе» и на одиночном — уходит на следующий шаг.
    #[test]
    fn enter_toggles_multi_options_and_advances_everywhere_else() {
        let mut app = app_for_ask();
        app.running = true;
        open_ask(
            &mut app,
            vec![
                question("Что затронуть?", true, &["Тесты", "Доки"], true),
                question("Второй?", false, &["B"], true),
            ],
        );

        app.ask_submit();
        assert_eq!(
            state(&mut app).step,
            0,
            "множественный: Enter не прыгает дальше"
        );
        assert!(
            state(&mut app).answers[0].checked[0],
            "Enter отметил вариант"
        );

        // Курсор на строке «свой ответ» — Enter уводит на следующий шаг.
        state(&mut app).answers[0].cursor = 2;
        app.ask_submit();
        assert_eq!(state(&mut app).step, 1, "со «своего ответа» — дальше");

        // Одиночный вопрос: Enter на варианте тоже уводит дальше, ничего не отмечая.
        let mut app = app_for_ask();
        app.running = true;
        open_ask(
            &mut app,
            vec![
                question("Провайдер?", false, &["Codex", "Claude"], true),
                question("Второй?", false, &["B"], true),
            ],
        );
        app.ask_submit();
        assert_eq!(state(&mut app).step, 1, "одиночный: Enter — следующий шаг");
    }
}
