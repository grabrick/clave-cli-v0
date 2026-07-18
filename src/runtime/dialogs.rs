use super::*;

/// Ввод при открытом inline-селекторе: навигация, отметки (multi), подтверждение,
/// «Свой вариант»/Esc → свободный ответ.
pub(crate) fn handle_ask_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        app.handle_ctrl_c();
        return;
    }
    match key.code {
        KeyCode::Up => app.ask_move(-1),
        KeyCode::Down => app.ask_move(1),
        // Переключение между вопросами (визард на несколько вопросов).
        KeyCode::Tab | KeyCode::Right => app.ask_next(),
        KeyCode::BackTab | KeyCode::Left => app.ask_prev(),
        KeyCode::Enter => app.ask_submit(),
        KeyCode::Esc => app.ask_cancel(),
        KeyCode::Backspace => app.ask_custom_backspace(),
        KeyCode::Char(ch) => {
            if app.ask_on_custom_row() {
                app.ask_custom_push(ch); // печать в поле «своего ответа»
            } else if ch == ' ' {
                app.ask_toggle(); // Space на варианте — отметить (multi)
            }
        }
        _ => {}
    }
}

pub(crate) fn handle_effort_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.effort_focus = app.effort_focus.saturating_sub(1),
        KeyCode::Down => {
            app.effort_focus = (app.effort_focus + 1).min(app.effort_picker_rows() - 1);
        }
        KeyCode::Left => app.adjust_effort_focus(-1),
        KeyCode::Right => app.adjust_effort_focus(1),
        KeyCode::Enter => {
            app.overlay = Overlay::None;
            app.effort_original = None;
            app.status = app.lang.choose("готов", "ready").to_string();
            app.save_current_config(true);
            app.push_command_result(format!("Set to {}", app.effort_summary()));
        }
        KeyCode::Esc => {
            if let Some(snapshot) = app.effort_original.take() {
                app.restore_effort_snapshot(snapshot);
            }
            app.overlay = Overlay::None;
            app.status = app.lang.choose("готов", "ready").to_string();
            app.push_command_result("Cancelled");
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.handle_ctrl_c();
        }
        _ => {}
    }
}

pub(crate) fn handle_chats_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.chats_index = app.chats_index.saturating_sub(1),
        KeyCode::Down => {
            let last = app.chats_picker.len().saturating_sub(1);
            app.chats_index = (app.chats_index + 1).min(last);
        }
        KeyCode::Enter => {
            let selected = app
                .chats_picker
                .get(app.chats_index)
                .map(|chat| chat.id.clone());
            app.overlay = Overlay::None;
            if let Some(id) = selected {
                app.resume_chat(&id);
            }
        }
        KeyCode::Esc => app.overlay = Overlay::None,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.handle_ctrl_c();
        }
        _ => {}
    }
}

pub(crate) fn handle_settings_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.adjust_settings_focus(-1),
        KeyCode::Down => app.adjust_settings_focus(1),
        KeyCode::Left => app.adjust_settings_value(-1),
        KeyCode::Right => app.adjust_settings_value(1),
        KeyCode::Enter => {
            app.overlay = Overlay::None;
            app.settings_original = None;
            app.status = app.lang.choose("готов", "ready").to_string();
            app.save_current_config(true);
            app.push_command_result(format!("Saved {}", app.settings_summary()));
        }
        KeyCode::Esc => {
            if let Some(snapshot) = app.settings_original.take() {
                app.restore_settings_snapshot(snapshot);
            }
            app.overlay = Overlay::None;
            app.status = app.lang.choose("готов", "ready").to_string();
            app.push_command_result("Cancelled");
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.handle_ctrl_c();
        }
        _ => {}
    }
}

#[cfg(test)]
pub(crate) mod keytest {
    use super::*;

    /// App для тестов клавиатуры. Собираем через `App::from_config` на своих временных путях
    /// и с `onboarding_done = true`: `App::new()` читал бы пользовательский конфиг и при
    /// невыполненном онбординге поднимал `Onboarding::new` — то есть настоящие auth-probe
    /// процессы codex/claude. Дальше фиксируем поля, которые читают обработчики клавиш.
    pub(crate) fn app_for_keys() -> App {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = env::temp_dir().join(format!(
            "clave-keys-{}-{}",
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
        app.chat_path = dir.join("chat.md");
        app.lang = Language::Ru;
        app.onboarding = None;
        app.overlay = Overlay::None;
        app.chat_mode = ChatMode::Discussion;
        app.input.clear();
        app.cursor = 0;
        app.transcript.clear();
        app.history.clear();
        app.history_index = None;
        app.history_draft = None;
        app.selected_suggestion = 0;
        app.pending_plan = None;
        app.plan_flow = PlanFlow::None;
        app.pending_messages.clear();
        app.running = false;
        app.should_quit = false;
        app.last_ctrl_c_at = None;
        app
    }

    /// App с активным гейтом плана (`pending_plan` + `!running`).
    pub(crate) fn app_with_plan_gate() -> App {
        let mut app = app_for_keys();
        app.pending_plan = Some(PendingPlan {
            task: "задача".to_string(),
            plan: "шаг 1".to_string(),
        });
        app
    }

    /// App с активным гейтом тандема (`tandem_gate` + `running`) и приёмником, чтобы
    /// проверить, какое решение ушло заблокированному воркеру.
    pub(crate) fn app_with_tandem_gate() -> (App, std::sync::mpsc::Receiver<TandemGate>) {
        let mut app = app_for_keys();
        let (tx, rx) = std::sync::mpsc::channel();
        app.running = true;
        app.tandem_gate = true;
        app.tandem_gate_tx = Some(tx);
        (app, rx)
    }

    #[test]
    fn tandem_gate_enter_approves_execution() {
        let (mut app, rx) = app_with_tandem_gate();
        handle_input_key(&mut app, key(KeyCode::Enter));
        assert_eq!(
            rx.try_recv().ok(),
            Some(TandemGate::Execute),
            "Enter на гейте → исполнить последнюю версию"
        );
        assert!(!app.tandem_gate, "гейт закрыт после решения");
    }

    #[test]
    fn tandem_gate_esc_aborts_without_writing() {
        let (mut app, rx) = app_with_tandem_gate();
        handle_input_key(&mut app, key(KeyCode::Esc));
        assert_eq!(
            rx.try_recv().ok(),
            Some(TandemGate::Abort),
            "Esc на гейте → отмена без записи"
        );
        assert!(!app.tandem_gate, "гейт закрыт после решения");
    }

    #[test]
    fn tandem_gate_ignores_ctrl_combinations() {
        // Ctrl+Enter НЕ одобряет исполнение: комбинации редактора/прерывания сюда не
        // относятся, иначе случайный Ctrl+Enter молча запускал бы запись.
        let (mut app, rx) = app_with_tandem_gate();
        handle_input_key(&mut app, ctrl(KeyCode::Enter));
        assert!(rx.try_recv().is_err(), "Ctrl+Enter решение не шлёт");
        assert!(app.tandem_gate, "Ctrl+Enter гейт не закрывает");
    }

    /// App с активным ВВОД-гейтом тандема + приёмники ответа и отмены.
    pub(crate) fn app_with_tandem_input_gate() -> (
        App,
        std::sync::mpsc::Receiver<String>,
        std::sync::mpsc::Receiver<()>,
    ) {
        let mut app = app_for_keys();
        let (in_tx, in_rx) = std::sync::mpsc::channel();
        let (cancel_tx, cancel_rx) = std::sync::mpsc::channel();
        app.running = true;
        app.tandem_input_gate = true;
        app.tandem_input_tx = Some(in_tx);
        app.cancel_tx = Some(cancel_tx);
        (app, in_rx, cancel_rx)
    }

    #[test]
    fn tandem_input_gate_enter_sends_typed_answer() {
        let (mut app, in_rx, _cancel_rx) = app_with_tandem_input_gate();
        app.input = "почини баг в X".to_string();
        app.cursor = app.input.len();
        handle_input_key(&mut app, key(KeyCode::Enter));
        assert_eq!(
            in_rx.try_recv().ok().as_deref(),
            Some("почини баг в X"),
            "Enter шлёт набранный ответ воркеру"
        );
        assert!(!app.tandem_input_gate, "гейт закрыт после ответа");
        assert!(app.input.is_empty(), "инпут очищен");
    }

    #[test]
    fn tandem_input_gate_empty_answer_is_ignored() {
        // Пустой ответ не отправляем — ждём текст (иначе воркер получил бы пустую строку).
        let (mut app, in_rx, _cancel_rx) = app_with_tandem_input_gate();
        app.input = "   ".to_string();
        handle_input_key(&mut app, key(KeyCode::Enter));
        assert!(in_rx.try_recv().is_err(), "пустой ответ не уходит");
        assert!(app.tandem_input_gate, "гейт остаётся открыт");
    }

    #[test]
    fn tandem_input_gate_esc_cancels_tandem() {
        let (mut app, _in_rx, cancel_rx) = app_with_tandem_input_gate();
        handle_input_key(&mut app, key(KeyCode::Esc));
        assert!(cancel_rx.try_recv().is_ok(), "Esc отменяет тандем");
        assert!(!app.tandem_input_gate, "гейт закрыт");
    }

    #[test]
    fn tandem_input_gate_passes_typing_through() {
        // Обычные символы на ввод-гейте — это НАБОР ответа, а не спецклавиши.
        let (mut app, in_rx, _cancel_rx) = app_with_tandem_input_gate();
        handle_input_key(&mut app, key(KeyCode::Char('a')));
        assert_eq!(app.input, "a", "символ ушёл в набор ответа");
        assert!(in_rx.try_recv().is_err(), "набор ничего не отправляет");
        assert!(app.tandem_input_gate, "гейт открыт, пока печатаешь");
    }

    pub(crate) fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    pub(crate) fn key_with(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    pub(crate) fn ctrl(code: KeyCode) -> KeyEvent {
        key_with(code, KeyModifiers::CONTROL)
    }

    pub(crate) fn alt(code: KeyCode) -> KeyEvent {
        key_with(code, KeyModifiers::ALT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::keytest::*;

    #[test]
    fn login_message_tells_the_three_outcomes_apart() {
        // Исходов три, и путать их нельзя: человек пойдёт чинить не то.
        let ok = login_message(true, 0, Language::Ru);
        assert!(ok.contains("готова"), "всё готово: {ok}");

        // Команда отработала (код 0), но нужных аккаунтов ещё не все.
        let partial = login_message(false, 0, Language::Ru);
        assert!(
            partial.contains("не все готовы"),
            "логин прошёл, но не всё: {partial}"
        );

        // Сама команда упала — это ДРУГОЕ, и текст обязан отличаться.
        let failed = login_message(false, 1, Language::Ru);
        assert!(failed.contains("ошибкой"), "команда упала: {failed}");
        assert_ne!(
            partial, failed,
            "«логин прошёл, но не всё» и «команда упала» — разные беды, и путать их нельзя"
        );

        assert!(login_message(true, 0, Language::En).contains("ready"));
    }

    // ─────────────────────────── СЕЛЕКТОР И ДИСПЕТЧЕР ───────────────────────────
    //
    // Хелперы селектора (`ask_question`, `app_with_ask`, `ask_state`) лежат ниже, рядом с
    // тестами `handle_ask_key`.

    #[test]
    fn an_open_selector_still_receives_keys() {
        // Условие «клавиша до-печатала прозу И открыла селектор → съесть её» держится на И.
        // С ИЛИ оно срабатывало бы от одного лишь открытого селектора — и тот перестал бы
        // отвечать на клавиши ВООБЩЕ: стрелки не двигают выбор, Enter не подтверждает.
        //
        // Идём через `handle_key` (диспетчер), а не через `handle_ask_key` напрямую: проверяется
        // именно МАРШРУТ до селектора.
        let mut app = app_with_ask(vec![ask_question(
            "Что делаем?",
            false,
            &["первый", "второй"],
        )]);
        assert_eq!(
            ask_state(&app).answers[0].cursor,
            0,
            "старт на первом варианте"
        );

        handle_key(&mut app, key(KeyCode::Down));

        assert_ne!(
            ask_state(&app).answers[0].cursor,
            0,
            "открытый селектор обязан отвечать на клавиши — иначе выбрать в нём нельзя ничего"
        );
    }

    // ─────────────────────────── ПЕТЛЯ СОБЫТИЙ ───────────────────────────

    /// Режим задаём явно: от него зависят и число строк пикера, и что двигают ←/→.
    fn app_for_effort(mode: Mode) -> App {
        let mut app = app_for_keys();
        app.overlay = Overlay::Effort;
        app.mode = mode;
        app.linked_effort_split = false;
        app.effort_focus = 0;
        app.effort_index = effort_index_for("high");
        app.codex_effort_index = effort_index_for("high");
        app.claude_effort_index = effort_index_for("high");
        app.effort_original = Some(app.effort_snapshot());
        app
    }

    fn app_for_settings() -> App {
        let mut app = app_for_keys();
        app.overlay = Overlay::Settings;
        app.settings_focus = 0;
        app.rounds = 5;
        app.theme = Theme::Purple;
        app.settings_original = Some(app.settings_snapshot());
        app
    }

    fn app_for_chats() -> App {
        let mut app = app_for_keys();
        app.overlay = Overlay::Chats;
        app.chats_index = 0;
        app.chats_picker = ["chat-one", "chat-two"]
            .iter()
            .map(|id| ChatSummary {
                id: (*id).to_string(),
                title: (*id).to_string(),
                lines: 3,
                modified: SystemTime::UNIX_EPOCH,
            })
            .collect();
        app
    }

    fn ask_question(question: &str, multi: bool, labels: &[&str]) -> AskQuestion {
        AskQuestion {
            question: question.to_string(),
            multi,
            options: labels
                .iter()
                .map(|label| AskOption {
                    label: (*label).to_string(),
                    note: None,
                })
                .collect(),
            allow_custom: true,
        }
    }

    /// running = true: `ask_submit` уходит в `start_chat`, который в этом состоянии кладёт
    /// сообщение в очередь и НЕ поднимает провайдер.
    fn app_with_ask(questions: Vec<AskQuestion>) -> App {
        let mut app = app_for_keys();
        app.running = true;
        app.ask_prompt_pending = Some(AskPrompt { questions });
        app.open_pending_ask();
        app
    }

    fn ask_state(app: &App) -> &AskState {
        app.ask.as_ref().expect("селектор открыт")
    }

    fn transcript_has(app: &App, needle: &str) -> bool {
        app.transcript.iter().any(|line| line.contains(needle))
    }

    // ───────────────────────── handle_effort_key ─────────────────────────

    #[test]
    fn effort_down_stops_at_last_row() {
        // ClaudeCodex без раздельного усилия — ровно две строки пикера.
        let mut app = app_for_effort(Mode::ClaudeCodex);
        handle_effort_key(&mut app, key(KeyCode::Down));
        assert_eq!(app.effort_focus, 1, "↓ переводит фокус на вторую строку");
        handle_effort_key(&mut app, key(KeyCode::Down));
        assert_eq!(app.effort_focus, 1, "ниже последней строки фокус не уходит");
    }

    #[test]
    fn effort_up_moves_focus_back() {
        let mut app = app_for_effort(Mode::ClaudeCodex);
        app.effort_focus = 1;
        handle_effort_key(&mut app, key(KeyCode::Up));
        assert_eq!(app.effort_focus, 0);
        handle_effort_key(&mut app, key(KeyCode::Up));
        assert_eq!(app.effort_focus, 0, "выше первой строки фокус не уходит");
    }

    #[test]
    fn effort_left_and_right_change_effort() {
        let mut down = app_for_effort(Mode::ClaudeOnly);
        handle_effort_key(&mut down, key(KeyCode::Left));
        assert_eq!(
            effort_label(down.claude_effort_index),
            "medium",
            "← ослабляет"
        );

        let mut up = app_for_effort(Mode::ClaudeOnly);
        handle_effort_key(&mut up, key(KeyCode::Right));
        assert_eq!(effort_label(up.claude_effort_index), "max", "→ усиливает");
    }

    #[test]
    fn effort_enter_saves_and_closes() {
        let mut app = app_for_effort(Mode::ClaudeOnly);
        handle_effort_key(&mut app, key(KeyCode::Right));
        handle_effort_key(&mut app, key(KeyCode::Enter));
        assert_eq!(app.overlay, Overlay::None);
        assert!(
            app.effort_original.is_none(),
            "снимок отпущен — правка принята"
        );
        assert_eq!(app.status, "готов");
        assert_eq!(
            effort_label(app.claude_effort_index),
            "max",
            "Enter не откатывает"
        );
        assert!(
            transcript_has(&app, "Set to"),
            "лента: {:?}",
            app.transcript
        );
        assert!(app.config_path.exists(), "Enter сохраняет конфиг");
    }

    #[test]
    fn effort_esc_restores_snapshot() {
        let mut app = app_for_effort(Mode::ClaudeOnly);
        app.claude_effort_index = effort_index_for("low");
        handle_effort_key(&mut app, key(KeyCode::Esc));
        assert_eq!(
            effort_label(app.claude_effort_index),
            "high",
            "Esc возвращает усилие из снимка"
        );
        assert!(app.effort_original.is_none());
        assert_eq!(app.overlay, Overlay::None);
        assert_eq!(app.status, "готов");
        assert!(
            transcript_has(&app, "Cancelled"),
            "лента: {:?}",
            app.transcript
        );
    }

    #[test]
    fn effort_quits_only_on_double_ctrl_c() {
        let mut app = app_for_effort(Mode::ClaudeOnly);
        handle_effort_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(!app.should_quit, "одиночный Ctrl+C не выходит");
        handle_effort_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(app.should_quit, "двойной Ctrl+C выходит");

        let mut plain = app_for_effort(Mode::ClaudeOnly);
        handle_effort_key(&mut plain, key(KeyCode::Char('c')));
        handle_effort_key(&mut plain, key(KeyCode::Char('c')));
        assert!(!plain.should_quit, "простая «c» — не Ctrl+C");
        assert!(
            plain.last_ctrl_c_at.is_none(),
            "простая «c» не считается за Ctrl+C"
        );
    }

    // ─────────────────── handle_onboarding_settings_key ───────────────────

    #[test]
    fn ask_down_and_up_wrap_over_rows() {
        let mut app = app_with_ask(vec![ask_question(
            "Продолжить?",
            false,
            &["Да", "Нет", "Позже"],
        )]);
        handle_ask_key(&mut app, key(KeyCode::Down));
        assert_eq!(ask_state(&app).answers[0].cursor, 1, "↓ идёт вниз");

        let mut up = app_with_ask(vec![ask_question(
            "Продолжить?",
            false,
            &["Да", "Нет", "Позже"],
        )]);
        handle_ask_key(&mut up, key(KeyCode::Up));
        assert_eq!(
            ask_state(&up).answers[0].cursor,
            3,
            "↑ с первой строки заворачивает на «свой ответ»"
        );
    }

    #[test]
    fn ask_tab_and_right_go_to_next_question() {
        for code in [KeyCode::Tab, KeyCode::Right] {
            let mut app = app_with_ask(vec![
                ask_question("Первый?", false, &["Да"]),
                ask_question("Второй?", false, &["Да"]),
            ]);
            handle_ask_key(&mut app, key(code));
            assert_eq!(
                ask_state(&app).step,
                1,
                "{code:?} ведёт к следующему вопросу"
            );
        }
    }

    #[test]
    fn ask_backtab_and_left_go_back() {
        for code in [KeyCode::BackTab, KeyCode::Left] {
            let mut app = app_with_ask(vec![
                ask_question("Первый?", false, &["Да"]),
                ask_question("Второй?", false, &["Да"]),
            ]);
            handle_ask_key(&mut app, key(KeyCode::Tab));
            handle_ask_key(&mut app, key(code));
            assert_eq!(
                ask_state(&app).step,
                0,
                "{code:?} возвращает к прошлому вопросу"
            );
        }
    }

    #[test]
    fn ask_enter_submits_single_question() {
        let mut app = app_with_ask(vec![ask_question("Продолжить?", false, &["Да", "Нет"])]);
        handle_ask_key(&mut app, key(KeyCode::Enter));
        assert!(app.ask.is_none(), "Enter закрывает селектор");
        let queued = app.pending_messages.front().expect("сообщение в очереди");
        assert!(
            queued.contains("«Да»"),
            "отправлен выбранный вариант: {queued}"
        );
    }

    #[test]
    fn ask_esc_closes_without_sending() {
        let mut app = app_with_ask(vec![ask_question("Продолжить?", false, &["Да", "Нет"])]);
        handle_ask_key(&mut app, key(KeyCode::Esc));
        assert!(app.ask.is_none());
        assert_eq!(app.status, "закрыто");
        assert!(app.pending_messages.is_empty(), "Esc ничего не отправляет");
    }

    #[test]
    fn ask_backspace_edits_custom_answer() {
        let mut app = app_with_ask(vec![ask_question("Продолжить?", false, &["Да", "Нет"])]);
        {
            let state = app.ask.as_mut().expect("селектор открыт");
            state.answers[0].cursor = 2; // строка «свой ответ»
            state.answers[0].custom = "ab".to_string();
        }
        handle_ask_key(&mut app, key(KeyCode::Backspace));
        assert_eq!(ask_state(&app).answers[0].custom, "a");
    }

    #[test]
    fn ask_plain_char_types_into_custom_answer() {
        // Именно 'c': мутант `&&` → `||` увёл бы её в ветку Ctrl+C с ранним return.
        let mut app = app_with_ask(vec![ask_question("Продолжить?", false, &["Да", "Нет"])]);
        app.ask.as_mut().expect("селектор открыт").answers[0].cursor = 2;
        handle_ask_key(&mut app, key(KeyCode::Char('c')));
        assert_eq!(ask_state(&app).answers[0].custom, "c");
        assert!(app.last_ctrl_c_at.is_none(), "простая «c» — не Ctrl+C");
    }

    #[test]
    fn ask_space_toggles_option_but_other_chars_do_not() {
        let mut space = app_with_ask(vec![ask_question("Что взять?", true, &["Да", "Нет"])]);
        handle_ask_key(&mut space, key(KeyCode::Char(' ')));
        assert!(
            ask_state(&space).answers[0].checked[0],
            "Space отмечает вариант"
        );

        let mut other = app_with_ask(vec![ask_question("Что взять?", true, &["Да", "Нет"])]);
        handle_ask_key(&mut other, key(KeyCode::Char('x')));
        assert!(
            !ask_state(&other).answers[0].checked[0],
            "любой другой символ на варианте отметку не ставит"
        );
    }

    #[test]
    fn ask_quits_on_double_ctrl_c() {
        let mut app = app_with_ask(vec![ask_question("Продолжить?", false, &["Да"])]);
        handle_ask_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(!app.should_quit);
        handle_ask_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(app.should_quit, "двойной Ctrl+C выходит");
    }

    // ───────────────────────── handle_settings_key ─────────────────────────

    #[test]
    fn settings_up_and_down_move_focus() {
        let mut app = app_for_settings();
        handle_settings_key(&mut app, key(KeyCode::Down));
        assert_eq!(app.settings_focus, 1, "↓ идёт вниз по строкам");

        app.settings_focus = 3;
        handle_settings_key(&mut app, key(KeyCode::Up));
        assert_eq!(app.settings_focus, 2, "↑ идёт вверх");
    }

    #[test]
    fn settings_left_and_right_change_rounds() {
        let mut less = app_for_settings();
        less.settings_focus = 4; // строка раундов
        handle_settings_key(&mut less, key(KeyCode::Left));
        assert_eq!(less.rounds, 4, "← уменьшает раунды");

        let mut more = app_for_settings();
        more.settings_focus = 4;
        handle_settings_key(&mut more, key(KeyCode::Right));
        assert_eq!(more.rounds, 6, "→ увеличивает раунды");
    }

    #[test]
    fn settings_enter_saves_and_closes() {
        let mut app = app_for_settings();
        app.settings_focus = 4;
        handle_settings_key(&mut app, key(KeyCode::Right));
        handle_settings_key(&mut app, key(KeyCode::Enter));
        assert_eq!(app.overlay, Overlay::None);
        assert!(
            app.settings_original.is_none(),
            "снимок отпущен — правка принята"
        );
        assert_eq!(app.rounds, 6, "Enter не откатывает значение");
        assert_eq!(app.status, "готов");
        assert!(transcript_has(&app, "Saved"), "лента: {:?}", app.transcript);
        assert!(app.config_path.exists(), "Enter сохраняет конфиг");
    }

    #[test]
    fn settings_esc_restores_snapshot() {
        let mut app = app_for_settings();
        app.theme = Theme::Amber;
        app.rounds = 9;
        handle_settings_key(&mut app, key(KeyCode::Esc));
        assert_eq!(app.theme, Theme::Purple, "Esc возвращает тему из снимка");
        assert_eq!(app.rounds, 5, "Esc возвращает раунды из снимка");
        assert!(app.settings_original.is_none());
        assert_eq!(app.overlay, Overlay::None);
        assert_eq!(app.status, "готов");
        assert!(
            transcript_has(&app, "Cancelled"),
            "лента: {:?}",
            app.transcript
        );
    }

    #[test]
    fn settings_quits_only_on_double_ctrl_c() {
        let mut app = app_for_settings();
        handle_settings_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(!app.should_quit, "одиночный Ctrl+C не выходит");
        handle_settings_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(app.should_quit, "двойной Ctrl+C выходит");

        let mut plain = app_for_settings();
        handle_settings_key(&mut plain, key(KeyCode::Char('c')));
        handle_settings_key(&mut plain, key(KeyCode::Char('c')));
        assert!(!plain.should_quit, "простая «c» — не Ctrl+C");
        assert!(plain.last_ctrl_c_at.is_none());
    }

    // ─────────────────── adjust_onboarding_setting ───────────────────

    #[test]
    fn chats_down_stops_at_last_chat() {
        let mut app = app_for_chats();
        handle_chats_key(&mut app, key(KeyCode::Down));
        assert_eq!(app.chats_index, 1);
        handle_chats_key(&mut app, key(KeyCode::Down));
        assert_eq!(app.chats_index, 1, "ниже последнего чата не уходит");
    }

    #[test]
    fn chats_up_moves_back() {
        let mut app = app_for_chats();
        app.chats_index = 1;
        handle_chats_key(&mut app, key(KeyCode::Up));
        assert_eq!(app.chats_index, 0);
    }

    #[test]
    fn chats_enter_closes_and_resumes_selected() {
        // Файла чата нет — resume_chat отвечает «Чат не найден.». Это и доказывает,
        // что Enter действительно позвал resume_chat, ничего при этом не запуская.
        let mut app = app_for_chats();
        handle_chats_key(&mut app, key(KeyCode::Enter));
        assert_eq!(app.overlay, Overlay::None);
        assert!(
            transcript_has(&app, "Чат не найден."),
            "Enter восстанавливает выбранный чат: {:?}",
            app.transcript
        );
    }

    #[test]
    fn chats_esc_closes_without_resume() {
        let mut app = app_for_chats();
        handle_chats_key(&mut app, key(KeyCode::Esc));
        assert_eq!(app.overlay, Overlay::None);
        assert!(app.transcript.is_empty(), "Esc чат не восстанавливает");
    }

    #[test]
    fn chats_quits_only_on_double_ctrl_c() {
        let mut app = app_for_chats();
        handle_chats_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(!app.should_quit, "одиночный Ctrl+C не выходит");
        handle_chats_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(app.should_quit, "двойной Ctrl+C выходит");

        let mut plain = app_for_chats();
        handle_chats_key(&mut plain, key(KeyCode::Char('c')));
        handle_chats_key(&mut plain, key(KeyCode::Char('c')));
        assert!(!plain.should_quit, "простая «c» — не Ctrl+C");
        assert!(plain.last_ctrl_c_at.is_none());
    }

    // ─────────────────── handle_onboarding_provider_key ───────────────────
}
