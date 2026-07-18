use super::*;

pub(crate) fn handle_input_key(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    // Ввод-гейт тандема «нужны уточнения»: воркер ждёт текстовый ответ. Печать/навигация
    // идут как обычно (набираешь ответ), Enter — отправить его воркеру, Esc — отменить тандем.
    if app.tandem_input_gate_active() && !ctrl && !alt {
        match key.code {
            KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                app.tandem_submit_input();
                return;
            }
            KeyCode::Esc => {
                app.tandem_input_cancel();
                return;
            }
            _ => {} // символы/навигация — обычным путём (набор ответа)
        }
    }

    // Гейт тандема «нет консенсуса»: воркер жив (running=true) и заблокирован в ожидании.
    // Enter — исполнить последнюю версию, Esc — отмена. Прочие клавиши пропускаем насквозь
    // (скролл ленты, чтобы перечитать дебаты перед решением); Ctrl+C для полной отмены идёт
    // своим путём — он ctrl и сюда не попадает.
    if app.tandem_gate_active() && !ctrl && !alt {
        match key.code {
            KeyCode::Enter => {
                app.tandem_gate_approve();
                return;
            }
            KeyCode::Esc => {
                app.tandem_gate_abort();
                return;
            }
            _ => {}
        }
    }

    // Гейт плана: Enter/Esc имеют особую семантику; остальное — обычный ввод
    // (набор замечания для доработки). Ctrl/Alt-комбинации не перехватываем.
    if app.plan_gate_active() && !ctrl && !alt {
        match key.code {
            KeyCode::Enter => {
                app.submit_plan_gate();
                return;
            }
            KeyCode::Esc => {
                app.cancel_plan();
                return;
            }
            KeyCode::BackTab => return, // режим не меняем, пока открыт гейт
            _ => {}
        }
    }

    if ctrl {
        match key.code {
            KeyCode::Char('c') => app.handle_ctrl_c(),
            KeyCode::Char('j') => app.insert_newline(),
            KeyCode::Char('m') => app.submit_input(),
            KeyCode::Char('a') => app.move_line_start(),
            KeyCode::Char('e') => app.move_line_end(),
            KeyCode::Char('b') => app.move_left(),
            KeyCode::Char('f') => app.move_right(),
            KeyCode::Char('p') => app.history_prev(),
            KeyCode::Char('n') => app.history_next(),
            KeyCode::Char('u') => app.kill_before_cursor(),
            KeyCode::Char('k') => app.kill_after_cursor(),
            KeyCode::Char('w') => app.delete_word_back(),
            KeyCode::Char('d') => app.delete(),
            KeyCode::Char('r') => app.open_search(),
            KeyCode::Left => app.move_word_left(),
            KeyCode::Right => app.move_word_right(),
            KeyCode::Backspace => app.delete_word_back(),
            KeyCode::Delete => app.delete_word_forward(),
            KeyCode::Home => app.cursor = 0,
            KeyCode::End => app.cursor = app.input.len(),
            _ => {}
        }
        return;
    }

    if alt {
        match key.code {
            // Alt/Option+Enter — перенос строки (надёжно различается во всех терминалах).
            KeyCode::Enter => app.insert_newline(),
            KeyCode::Left | KeyCode::Char('b') => app.move_word_left(),
            KeyCode::Right | KeyCode::Char('f') => app.move_word_right(),
            KeyCode::Backspace => app.delete_word_back(),
            KeyCode::Delete | KeyCode::Char('d') => app.delete_word_forward(),
            _ => {}
        }
        return;
    }

    match key.code {
        // Shift+Enter — перенос строки (где терминал сообщает модификатор); обычный
        // Enter отправляет. Ещё варианты переноса: Alt/Option+Enter и Ctrl+J.
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => app.insert_newline(),
        KeyCode::Enter => app.submit_input(),
        KeyCode::Tab => app.complete_command(),
        KeyCode::BackTab => {
            // Смена режима переживает перезапуск: сохраняем сразу (двойной Ctrl+C мог бы
            // выйти мимо сохранения), а не полагаемся на save где-то ещё.
            app.chat_mode = app.chat_mode.next();
            app.save_current_config(true);
        }
        KeyCode::Backspace => app.backspace(),
        KeyCode::Delete => app.delete(),
        KeyCode::Left => app.move_left(),
        KeyCode::Right => app.move_right(),
        // Стрелки умные: в многострочном вводе двигают курсор по строкам, на краю —
        // история (с сохранением черновика). Скролл ленты — нативный (колесо/скролл).
        KeyCode::Up => app.input_up(),
        KeyCode::Down => app.input_down(),
        KeyCode::Home => app.move_line_start(),
        KeyCode::End => app.move_line_end(),
        KeyCode::Esc => {
            app.input.clear();
            app.cursor = 0;
            app.history_index = None;
            app.selected_suggestion = 0;
        }
        KeyCode::Char('?') if app.input.is_empty() => app.overlay = Overlay::Shortcuts,
        KeyCode::Char(ch) if !ch.is_control() => app.insert_char(ch),
        _ => {}
    }
}

pub(crate) fn handle_shortcuts_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        app.handle_ctrl_c();
        return;
    }
    app.overlay = Overlay::None;
}

pub(crate) fn handle_search_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if matches!(key.code, KeyCode::Char('c')) {
            app.handle_ctrl_c();
        }
        return;
    }
    match key.code {
        KeyCode::Esc => app.close_search(),
        KeyCode::Enter | KeyCode::Down => app.search_step(1),
        KeyCode::Up => app.search_step(-1),
        KeyCode::Backspace => app.search_backspace(),
        KeyCode::Char(ch) if !ch.is_control() => app.search_input(ch),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::keytest::*;

    #[test]
    fn shift_enter_inserts_newline_instead_of_submitting() {
        let mut app = app_for_keys();
        app.running = true; // страховка: даже при мутации guard'а отправка уйдёт в очередь
        app.input = "abc".to_string();
        app.cursor = app.input.len();
        handle_input_key(&mut app, key_with(KeyCode::Enter, KeyModifiers::SHIFT));
        assert_eq!(app.input, "abc\n");
        assert!(app.pending_messages.is_empty(), "Shift+Enter не отправляет");
    }

    // ───────────────────────── Ctrl-ярус ─────────────────────────

    #[test]
    fn ctrl_c_twice_quits() {
        let mut app = app_for_keys();
        handle_input_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(!app.should_quit, "первый Ctrl+C только предупреждает");
        handle_input_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(app.should_quit, "двойной Ctrl+C выходит");
    }

    #[test]
    fn ctrl_j_inserts_newline() {
        let mut app = app_for_keys();
        app.input = "ab".to_string();
        app.cursor = 2;
        handle_input_key(&mut app, ctrl(KeyCode::Char('j')));
        assert_eq!(app.input, "ab\n");
    }

    #[test]
    fn ctrl_m_submits_input() {
        let mut app = app_for_keys();
        app.running = true;
        app.input = "ping".to_string();
        app.cursor = app.input.len();
        handle_input_key(&mut app, ctrl(KeyCode::Char('m')));
        assert!(app.input.is_empty());
        assert_eq!(
            app.pending_messages.front().map(String::as_str),
            Some("ping")
        );
    }

    #[test]
    fn ctrl_a_and_ctrl_e_jump_to_line_edges() {
        let mut app = app_for_keys();
        app.input = "abc".to_string();
        app.cursor = 2;
        handle_input_key(&mut app, ctrl(KeyCode::Char('a')));
        assert_eq!(app.cursor, 0);
        handle_input_key(&mut app, ctrl(KeyCode::Char('e')));
        assert_eq!(app.cursor, 3);
    }

    #[test]
    fn ctrl_b_and_ctrl_f_move_by_char() {
        let mut app = app_for_keys();
        app.input = "abc".to_string();
        app.cursor = 2;
        handle_input_key(&mut app, ctrl(KeyCode::Char('b')));
        assert_eq!(app.cursor, 1);
        handle_input_key(&mut app, ctrl(KeyCode::Char('f')));
        assert_eq!(app.cursor, 2);
    }

    #[test]
    fn ctrl_p_and_ctrl_n_walk_history() {
        let mut app = app_for_keys();
        app.history = vec!["one".to_string(), "two".to_string()];
        app.input = "draft".to_string();
        app.cursor = app.input.len();
        handle_input_key(&mut app, ctrl(KeyCode::Char('p')));
        assert_eq!(app.input, "two", "Ctrl+P — последняя команда истории");
        handle_input_key(&mut app, ctrl(KeyCode::Char('n')));
        assert_eq!(app.input, "draft", "Ctrl+N возвращает черновик");
    }

    #[test]
    fn ctrl_u_and_ctrl_k_kill_around_cursor() {
        let mut before = app_for_keys();
        before.input = "abcdef".to_string();
        before.cursor = 3;
        handle_input_key(&mut before, ctrl(KeyCode::Char('u')));
        assert_eq!(before.input, "def");
        assert_eq!(before.cursor, 0);

        let mut after = app_for_keys();
        after.input = "abcdef".to_string();
        after.cursor = 3;
        handle_input_key(&mut after, ctrl(KeyCode::Char('k')));
        assert_eq!(after.input, "abc");
    }

    #[test]
    fn ctrl_w_deletes_word_back() {
        let mut app = app_for_keys();
        app.input = "hello world".to_string();
        app.cursor = app.input.len();
        handle_input_key(&mut app, ctrl(KeyCode::Char('w')));
        assert_eq!(app.input, "hello ");
    }

    #[test]
    fn ctrl_backspace_deletes_word_back() {
        let mut app = app_for_keys();
        app.input = "hello world".to_string();
        app.cursor = app.input.len();
        handle_input_key(&mut app, ctrl(KeyCode::Backspace));
        assert_eq!(app.input, "hello ");
    }

    #[test]
    fn ctrl_d_deletes_char_under_cursor() {
        let mut app = app_for_keys();
        app.input = "abc".to_string();
        app.cursor = 0;
        handle_input_key(&mut app, ctrl(KeyCode::Char('d')));
        assert_eq!(app.input, "bc");
    }

    #[test]
    fn ctrl_r_opens_search() {
        let mut app = app_for_keys();
        handle_input_key(&mut app, ctrl(KeyCode::Char('r')));
        assert_eq!(app.overlay, Overlay::Search);
    }

    #[test]
    fn ctrl_arrows_move_by_word() {
        let mut app = app_for_keys();
        app.input = "one two".to_string();
        app.cursor = app.input.len();
        handle_input_key(&mut app, ctrl(KeyCode::Left));
        assert_eq!(app.cursor, 4, "курсор в начало слова «two»");
        handle_input_key(&mut app, ctrl(KeyCode::Right));
        assert_eq!(app.cursor, 7, "и обратно в конец слова");
    }

    #[test]
    fn ctrl_delete_deletes_word_forward() {
        let mut app = app_for_keys();
        app.input = "one two".to_string();
        app.cursor = 0;
        handle_input_key(&mut app, ctrl(KeyCode::Delete));
        assert_eq!(app.input, " two");
    }

    #[test]
    fn ctrl_home_and_ctrl_end_jump_to_input_edges() {
        let mut app = app_for_keys();
        app.input = "ab\ncd".to_string();
        app.cursor = 4;
        handle_input_key(&mut app, ctrl(KeyCode::Home));
        assert_eq!(app.cursor, 0, "Ctrl+Home — в самое начало ввода");
        handle_input_key(&mut app, ctrl(KeyCode::End));
        assert_eq!(app.cursor, app.input.len(), "Ctrl+End — в самый конец");
    }

    // ───────────────────────── Alt-ярус ─────────────────────────

    #[test]
    fn alt_enter_inserts_newline() {
        let mut app = app_for_keys();
        app.running = true;
        app.input = "abc".to_string();
        app.cursor = app.input.len();
        handle_input_key(&mut app, alt(KeyCode::Enter));
        assert_eq!(app.input, "abc\n");
        assert!(app.pending_messages.is_empty(), "Alt+Enter не отправляет");
    }

    #[test]
    fn alt_left_and_alt_b_move_word_left() {
        // Обе клавиши руки — на своём свежем состоянии, иначе успех первой замаскирует
        // выпавшую вторую.
        for code in [KeyCode::Left, KeyCode::Char('b')] {
            let mut app = app_for_keys();
            app.input = "one two".to_string();
            app.cursor = app.input.len();
            handle_input_key(&mut app, alt(code));
            assert_eq!(app.cursor, 4, "Alt+{code:?} — слово влево");
            assert_eq!(app.input, "one two", "текст не изменился");
        }
    }

    #[test]
    fn alt_right_and_alt_f_move_word_right() {
        for code in [KeyCode::Right, KeyCode::Char('f')] {
            let mut app = app_for_keys();
            app.input = "one two".to_string();
            app.cursor = 0;
            handle_input_key(&mut app, alt(code));
            assert_eq!(app.cursor, 3, "Alt+{code:?} — слово вправо");
            assert_eq!(app.input, "one two");
        }
    }

    #[test]
    fn alt_backspace_deletes_word_back() {
        let mut app = app_for_keys();
        app.input = "one two".to_string();
        app.cursor = app.input.len();
        handle_input_key(&mut app, alt(KeyCode::Backspace));
        assert_eq!(app.input, "one ");
    }

    #[test]
    fn alt_delete_and_alt_d_delete_word_forward() {
        for code in [KeyCode::Delete, KeyCode::Char('d')] {
            let mut app = app_for_keys();
            app.input = "one two".to_string();
            app.cursor = 0;
            handle_input_key(&mut app, alt(code));
            assert_eq!(app.input, " two", "Alt+{code:?} — слово вперёд");
        }
    }

    // ───────────────────────── голый ярус ─────────────────────────

    #[test]
    fn tab_completes_command() {
        let mut app = app_for_keys();
        app.input = "/brain".to_string();
        app.cursor = app.input.len();
        handle_input_key(&mut app, key(KeyCode::Tab));
        assert!(
            app.input.starts_with("/brainstorm"),
            "Tab дополняет команду: {}",
            app.input
        );
        assert_eq!(app.cursor, app.input.len());
    }

    #[test]
    fn backtab_switches_chat_mode() {
        let mut app = app_for_keys();
        handle_input_key(&mut app, key(KeyCode::BackTab));
        assert_eq!(app.chat_mode, ChatMode::Discussion.next());
        assert_ne!(app.chat_mode, ChatMode::Discussion);
    }

    #[test]
    fn backtab_persists_chat_mode_across_restart() {
        // Регресс: режим (Tandem) сбрасывался на Discussion при перезапуске — Shift+Tab не
        // сохранял конфиг. Теперь смена пишется на диск сразу и переживает выход.
        let mut app = app_for_keys();
        handle_input_key(&mut app, key(KeyCode::BackTab)); // Discussion → Plan
        assert_eq!(app.chat_mode, ChatMode::Plan);
        let reloaded = crate::storage::load_config(&app.config_path);
        assert_eq!(
            reloaded.chat_mode,
            ChatMode::Plan,
            "Shift+Tab сохранил режим на диск"
        );
    }

    #[test]
    fn backspace_and_delete_edit_around_cursor() {
        let mut back = app_for_keys();
        back.input = "abc".to_string();
        back.cursor = 2;
        handle_input_key(&mut back, key(KeyCode::Backspace));
        assert_eq!(back.input, "ac");
        assert_eq!(back.cursor, 1);

        let mut del = app_for_keys();
        del.input = "abc".to_string();
        del.cursor = 1;
        handle_input_key(&mut del, key(KeyCode::Delete));
        assert_eq!(del.input, "ac");
        assert_eq!(del.cursor, 1);
    }

    #[test]
    fn arrows_move_cursor_by_char() {
        let mut app = app_for_keys();
        app.input = "abc".to_string();
        app.cursor = 1;
        handle_input_key(&mut app, key(KeyCode::Right));
        assert_eq!(app.cursor, 2);
        handle_input_key(&mut app, key(KeyCode::Left));
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn up_and_down_walk_history_from_single_line() {
        // Инпут без «/» — иначе Up/Down листали бы палитру подсказок, а не историю.
        let mut app = app_for_keys();
        app.history = vec!["one".to_string(), "two".to_string()];
        app.input = "draft".to_string();
        app.cursor = app.input.len();
        handle_input_key(&mut app, key(KeyCode::Up));
        assert_eq!(app.input, "two");
        handle_input_key(&mut app, key(KeyCode::Down));
        assert_eq!(app.input, "draft", "черновик вернулся");
    }

    #[test]
    fn home_and_end_jump_within_current_line() {
        let mut app = app_for_keys();
        app.input = "ab\ncd".to_string();
        app.cursor = 4;
        handle_input_key(&mut app, key(KeyCode::Home));
        assert_eq!(app.cursor, 3, "Home — в начало ТЕКУЩЕЙ строки");
        handle_input_key(&mut app, key(KeyCode::End));
        assert_eq!(app.cursor, 5, "End — в конец текущей строки");
    }

    #[test]
    fn esc_clears_input() {
        let mut app = app_for_keys();
        app.input = "abc".to_string();
        app.cursor = 3;
        app.history = vec!["one".to_string()];
        app.history_index = Some(0);
        handle_input_key(&mut app, key(KeyCode::Esc));
        assert!(app.input.is_empty());
        assert_eq!(app.cursor, 0);
        assert!(app.history_index.is_none());
    }

    #[test]
    fn question_mark_opens_shortcuts_only_on_empty_input() {
        let mut empty = app_for_keys();
        handle_input_key(&mut empty, key(KeyCode::Char('?')));
        assert_eq!(empty.overlay, Overlay::Shortcuts);
        assert!(empty.input.is_empty(), "оверлей вместо ввода символа");

        let mut typed = app_for_keys();
        typed.input = "как".to_string();
        typed.cursor = typed.input.len();
        handle_input_key(&mut typed, key(KeyCode::Char('?')));
        assert_eq!(
            typed.overlay,
            Overlay::None,
            "внутри вопроса оверлей не лезет"
        );
        assert_eq!(typed.input, "как?");
    }

    #[test]
    fn printable_char_inserts_control_char_does_not() {
        let mut printable = app_for_keys();
        handle_input_key(&mut printable, key(KeyCode::Char('ы')));
        assert_eq!(printable.input, "ы");
        assert_eq!(
            printable.cursor,
            "ы".len(),
            "курсор шагнул на байты символа"
        );

        let mut control = app_for_keys();
        handle_input_key(&mut control, key(KeyCode::Char('\u{1}')));
        assert!(
            control.input.is_empty(),
            "управляющий символ в ввод не попадает"
        );
    }

    // ─────────────────────────── ВОЗВРАТ ТЕРМИНАЛА ───────────────────────────
    //
    // Мутационный прогон показал, что `restore_terminal` целиком заменяется пустышкой, и этого
    // не замечает НИ ОДИН тест. Цена такой поломки — не косметика: рендер прячет курсор через
    // Hide, и без явного Show после аварии пользователь остаётся с невидимым курсором в
    // raw-режиме. Печатаешь — не видно, Ctrl+C не работает; спасает только `reset` или закрыть
    // окно. И CI пропустил бы это молча.

    /// Три совпадения: на двух ↑ и ↓ давали бы один индекс, и тест не отличил бы
    /// шаг назад от шага вперёд.
    fn app_for_search() -> App {
        let mut app = app_for_keys();
        app.overlay = Overlay::Search;
        app.transcript = vec!["a1".into(), "a2".into(), "a3".into()];
        app.search_query = "a".to_string();
        app.search_index = 0;
        app
    }

    #[test]
    fn search_esc_closes_and_clears_query() {
        let mut app = app_for_search();
        handle_search_key(&mut app, key(KeyCode::Esc));
        assert_eq!(app.overlay, Overlay::None);
        assert!(app.search_query.is_empty(), "запрос очищен");
    }

    #[test]
    fn search_enter_and_down_step_forward() {
        for code in [KeyCode::Enter, KeyCode::Down] {
            let mut app = app_for_search();
            handle_search_key(&mut app, key(code));
            assert_eq!(app.search_index, 1, "{code:?} идёт к следующему совпадению");
        }
    }

    #[test]
    fn search_up_steps_backward() {
        let mut app = app_for_search();
        handle_search_key(&mut app, key(KeyCode::Up));
        assert_eq!(
            app.search_index, 2,
            "↑ с первого совпадения заворачивает на последнее"
        );
    }

    #[test]
    fn search_backspace_trims_query() {
        let mut app = app_for_search();
        app.search_query = "ab".to_string();
        handle_search_key(&mut app, key(KeyCode::Backspace));
        assert_eq!(app.search_query, "a");
    }

    #[test]
    fn search_types_printable_but_ignores_control_chars() {
        let mut app = app_for_search();
        handle_search_key(&mut app, key(KeyCode::Char('x')));
        assert_eq!(app.search_query, "ax", "печатный символ попадает в запрос");

        let mut control = app_for_search();
        handle_search_key(&mut control, key(KeyCode::Char('\u{1}')));
        assert_eq!(
            control.search_query, "a",
            "управляющий символ в запрос не попадает"
        );
    }

    // ───────────────────────── handle_chats_key ─────────────────────────

    #[test]
    fn shortcuts_ctrl_c_stays_open() {
        let mut app = app_for_keys();
        app.overlay = Overlay::Shortcuts;
        handle_shortcuts_key(&mut app, ctrl(KeyCode::Char('c')));
        assert_eq!(
            app.overlay,
            Overlay::Shortcuts,
            "Ctrl+C не закрывает подсказку"
        );
        assert!(app.last_ctrl_c_at.is_some(), "Ctrl+C учтён");
    }

    #[test]
    fn shortcuts_any_other_key_closes() {
        // Ctrl+X и простая «c» — обе половины условия: любая из них закрывает оверлей.
        for event in [ctrl(KeyCode::Char('x')), key(KeyCode::Char('c'))] {
            let mut app = app_for_keys();
            app.overlay = Overlay::Shortcuts;
            handle_shortcuts_key(&mut app, event);
            assert_eq!(app.overlay, Overlay::None, "{event:?} закрывает подсказку");
            assert!(app.last_ctrl_c_at.is_none(), "{event:?} — не Ctrl+C");
        }
    }

    // ───────────────────────── handle_onboarding_key ─────────────────────────
}
