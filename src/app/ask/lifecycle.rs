use super::*;

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

    /// Открывает селектор для ЗАКРЫТОГО вопроса тандема: тот же визард, но выбор пойдёт в
    /// паузу тандема (`feeds_tandem`), а не в новый чат-прогон.
    pub(crate) fn open_tandem_choice(&mut self, prompt: AskPrompt) {
        let mut state = AskState::new(prompt);
        state.feeds_tandem = true;
        self.ask = Some(state);
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
        let feeds_tandem = state.feeds_tandem;
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
        if feeds_tandem {
            self.resume_tandem_with(message);
        } else {
            self.start_chat(message);
        }
    }

    /// Собирает ответы на все вопросы в одно сообщение и отправляет модели.
    fn ask_send_all(&mut self) {
        let Some(state) = &self.ask else {
            return;
        };
        let feeds_tandem = state.feeds_tandem;
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
        if feeds_tandem {
            self.resume_tandem_with(message);
        } else {
            self.start_chat(message);
        }
    }

    /// Esc: закрыть селектор. Для тандемного выбора Esc отменяет тандем (мы на дебатах —
    /// файлы не тронуты); для локального вопроса приложения (напр. `/dev`) отвечать текстом
    /// некому — это отмена самого действия; для обычного — просто закрыть.
    pub(crate) fn ask_cancel(&mut self) {
        let feeds_tandem = self.ask.as_ref().is_some_and(|s| s.feeds_tandem);
        if self.ask.take().is_some() {
            if self.ask_intent.take().is_some() {
                self.push_system(self.lang.choose("⏹ /dev отменён.", "⏹ /dev cancelled."));
                self.status = self.lang.choose("отменено", "cancelled").to_string();
                return;
            }
            if feeds_tandem {
                self.tandem_input_cancel();
            }
            self.status = self.lang.choose("закрыто", "closed").to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ask::testkit::*;

    #[test]
    fn tandem_choice_selection_feeds_the_worker_not_a_new_chat() {
        // Тот же селектор, но открыт для тандема: выбор ОБЯЗАН уйти в заблокированный воркер
        // (tandem_input_tx), а не запустить новый чат-прогон через start_chat.
        let mut app = app_for_ask();
        let (in_tx, in_rx) = std::sync::mpsc::channel::<String>();
        app.tandem_input_tx = Some(in_tx);

        app.open_tandem_choice(AskPrompt {
            questions: vec![question("Тип?", false, &["фича", "багфикс"], true)],
        });
        assert!(
            state(&mut app).feeds_tandem,
            "селектор помечен как тандемный"
        );
        state(&mut app).answers[0].cursor = 1; // «багфикс»
        app.ask_submit();

        assert_eq!(
            in_rx.try_recv().ok().as_deref(),
            Some("Выбрано: «багфикс»"),
            "выбор питает паузу тандема, а не новый чат"
        );
        assert!(app.ask.is_none(), "селектор закрыт");
        assert!(
            app.pending_messages.is_empty() && !app.running,
            "нового чат-прогона не заводим"
        );
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

    #[test]
    fn reset_ask_closes_the_selector_and_forgets_the_intent() {
        let mut app = app_for_ask();
        open_ask(
            &mut app,
            vec![question("Зрение?", false, &["Нет", "Да"], false)],
        );
        app.ask_prompt_pending = Some(AskPrompt {
            questions: vec![question("Ещё?", false, &["A"], false)],
        });
        app.ask_intent = Some(AskIntent::DevVision {
            task: "почини".to_string(),
        });

        app.reset_ask();

        assert!(!app.ask_active(), "селектор закрыт");
        assert!(app.ask_prompt_pending.is_none());
        assert!(app.ask_intent.is_none());
    }

    /// Локальный вопрос приложения (зрение для `/dev`) обрабатывается сами и уходит в
    /// start_dev: селектор закрывается, а прогон стартует (здесь — упирается в busy).
    #[test]
    fn local_intent_answer_starts_dev_and_closes_the_selector() {
        let mut app = app_for_ask();
        app.running = true; // busy-preflight start_dev вместо живого прогона
        app.ask_intent = Some(AskIntent::DevVision {
            task: "почини футер".to_string(),
        });
        open_ask(
            &mut app,
            vec![question("Зрение?", false, &["Нет", "Да"], false)],
        );
        state(&mut app).answers[0].cursor = 1;

        app.ask_submit();

        assert!(!app.ask_active(), "локальный вопрос закрывается сам");
        assert!(app.ask_intent.is_none(), "намерение израсходовано");
        assert!(
            app.transcript
                .iter()
                .any(|line| line.contains("Clave уже выполняется")),
            "start_dev вызван: {:?}",
            app.transcript
        );
    }

    /// Строка «свой ответ» для локальных вопросов не поддерживается — прогон не стартует.
    #[test]
    fn local_intent_ignores_the_custom_row() {
        let mut app = app_for_ask();
        app.running = true;
        app.ask_intent = Some(AskIntent::DevVision {
            task: "почини футер".to_string(),
        });
        open_ask(
            &mut app,
            vec![question("Зрение?", false, &["Нет", "Да"], false)],
        );
        state(&mut app).answers[0].cursor = 2; // за пределами вариантов

        app.ask_submit();

        assert!(app.ask_active(), "селектор остаётся открыт");
        assert!(app.ask_intent.is_some(), "намерение не израсходовано");
        assert!(app.transcript.is_empty(), "прогон не стартовал");
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
