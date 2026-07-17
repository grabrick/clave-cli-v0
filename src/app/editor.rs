use super::*;

impl App {
    pub(crate) fn insert_char(&mut self, ch: char) {
        self.input.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
        self.history_index = None;
        self.selected_suggestion = 0;
    }

    pub(crate) fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    /// Вставка текста (bracketed paste): целиком на место курсора, с переносами
    /// строк и БЕЗ отправки. `\r\n` и одиночные `\r` нормализуем в `\n`.
    pub(crate) fn paste_into_input(&mut self, text: &str) {
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        if normalized.is_empty() {
            return;
        }
        self.input.insert_str(self.cursor, &normalized);
        self.cursor += normalized.len();
        self.history_index = None;
        self.selected_suggestion = 0;
    }

    pub(crate) fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }

        let prev = previous_boundary(&self.input, self.cursor);
        self.input.drain(prev..self.cursor);
        self.cursor = prev;
        self.history_index = None;
        self.selected_suggestion = 0;
    }

    pub(crate) fn delete(&mut self) {
        if self.cursor >= self.input.len() {
            return;
        }

        let next = next_boundary(&self.input, self.cursor);
        self.input.drain(self.cursor..next);
        self.history_index = None;
        self.selected_suggestion = 0;
    }

    pub(crate) fn move_left(&mut self) {
        self.cursor = previous_boundary(&self.input, self.cursor);
    }

    pub(crate) fn move_right(&mut self) {
        self.cursor = next_boundary(&self.input, self.cursor);
    }

    pub(crate) fn move_word_left(&mut self) {
        self.cursor = previous_word_boundary(&self.input, self.cursor);
    }

    pub(crate) fn move_word_right(&mut self) {
        self.cursor = next_word_boundary(&self.input, self.cursor);
    }

    pub(crate) fn move_line_start(&mut self) {
        self.cursor = line_start_boundary(&self.input, self.cursor);
    }

    pub(crate) fn move_line_end(&mut self) {
        self.cursor = line_end_boundary(&self.input, self.cursor);
    }

    pub(crate) fn delete_word_back(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = previous_word_boundary(&self.input, self.cursor);
        self.input.drain(start..self.cursor);
        self.cursor = start;
        self.history_index = None;
        self.selected_suggestion = 0;
    }

    pub(crate) fn delete_word_forward(&mut self) {
        if self.cursor >= self.input.len() {
            return;
        }
        let end = next_word_boundary(&self.input, self.cursor);
        self.input.drain(self.cursor..end);
        self.history_index = None;
        self.selected_suggestion = 0;
    }

    pub(crate) fn kill_before_cursor(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.input.drain(..self.cursor);
        self.cursor = 0;
        self.history_index = None;
        self.selected_suggestion = 0;
    }

    pub(crate) fn kill_after_cursor(&mut self) {
        if self.cursor >= self.input.len() {
            return;
        }
        self.input.drain(self.cursor..);
        self.history_index = None;
        self.selected_suggestion = 0;
    }

    /// Стрелка Вверх в инпуте. В многострочном вводе (курсор не на первой строке)
    /// двигает курсор на строку выше; на первой строке листает палитру подсказок
    /// или уходит в историю (с сохранением черновика).
    pub(crate) fn input_up(&mut self) {
        if self.history_index.is_none() {
            let suggestions = self.suggestions();
            if !suggestions.is_empty() {
                self.selected_suggestion = self.selected_suggestion.saturating_sub(1);
                return;
            }
            if line_start_boundary(&self.input, self.cursor) > 0 {
                self.move_cursor_up();
                return;
            }
        }
        self.history_prev();
    }

    /// Стрелка Вниз в инпуте — симметрично `input_up`.
    pub(crate) fn input_down(&mut self) {
        if self.history_index.is_none() {
            let suggestions = self.suggestions();
            if !suggestions.is_empty() {
                self.selected_suggestion =
                    (self.selected_suggestion + 1).min(suggestions.len() - 1);
                return;
            }
            if line_end_boundary(&self.input, self.cursor) < self.input.len() {
                self.move_cursor_down();
                return;
            }
        }
        self.history_next();
    }

    /// Курсор на строку выше, сохраняя визуальную колонку (в символах).
    pub(crate) fn move_cursor_up(&mut self) {
        let line_start = line_start_boundary(&self.input, self.cursor);
        if line_start == 0 {
            return;
        }
        let col = display_width(&self.input[line_start..self.cursor]);
        let prev_newline = line_start - 1;
        let prev_start = line_start_boundary(&self.input, prev_newline);
        self.cursor = byte_at_column(&self.input, prev_start, prev_newline, col);
    }

    /// Курсор на строку ниже, сохраняя визуальную колонку (в символах).
    pub(crate) fn move_cursor_down(&mut self) {
        let line_end = line_end_boundary(&self.input, self.cursor);
        if line_end >= self.input.len() {
            return;
        }
        let line_start = line_start_boundary(&self.input, self.cursor);
        let col = display_width(&self.input[line_start..self.cursor]);
        let next_start = line_end + 1;
        let next_end = line_end_boundary(&self.input, next_start);
        self.cursor = byte_at_column(&self.input, next_start, next_end, col);
    }

    /// История назад (Ctrl+P или Вверх на первой строке). При первом входе
    /// запоминает текущий ввод как черновик — чтобы вернуть его по `Down`.
    pub(crate) fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let next_index = match self.history_index {
            Some(index) => index.saturating_sub(1),
            None => {
                self.history_draft = Some(self.input.clone());
                self.history.len() - 1
            }
        };
        self.history_index = Some(next_index);
        self.input = self.history[next_index].clone();
        self.cursor = self.input.len();
    }

    /// История вперёд (Ctrl+N или Вниз на последней строке). За концом истории
    /// возвращает сохранённый черновик, а не очищает ввод.
    pub(crate) fn history_next(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };
        if index + 1 >= self.history.len() {
            self.history_index = None;
            self.input = self.history_draft.take().unwrap_or_default();
        } else {
            let next_index = index + 1;
            self.history_index = Some(next_index);
            self.input = self.history[next_index].clone();
        }
        self.cursor = self.input.len();
    }
}

/// Байтовая позиция `col`-го символа в `input[start..end]` (или `end`, если строка
/// короче). Держит «колонку» курсора при переходе между строками.
fn byte_at_column(input: &str, start: usize, end: usize, target_col: usize) -> usize {
    let mut pos = start;
    let mut col = 0usize;
    for ch in input[start..end].chars() {
        if col >= target_col {
            break;
        }
        col += char_display_width(ch);
        pos += ch.len_utf8();
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Изолированный App на временных путях: `App::new()` читал бы настоящий конфиг ~/.clave
    /// и будил бы auth-пробы — тесты стали бы флейкими и зависели бы от машины.
    fn editor_app() -> App {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "clave-editor-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::create_dir_all(&dir);
        App::from_config(
            AppConfig {
                onboarding_done: true,
                ..AppConfig::default()
            },
            dir.join("config.json"),
            dir.join("history"),
            dir.clone(),
        )
    }

    #[test]
    fn byte_at_column_uses_display_width() {
        // "aあb": a=col0, あ=cols1..2, b=col3. Колонка 3 → байт на 'b'.
        let s = "aあb";
        assert_eq!(byte_at_column(s, 0, s.len(), 3), "aあ".len());
        // Колонка внутри широкого символа «прыгает» за него (курсор не встаёт внутрь).
        assert_eq!(byte_at_column(s, 0, s.len(), 2), "aあ".len());
    }

    #[test]
    fn history_round_trip_restores_draft() {
        let mut app = editor_app();
        app.history = vec!["old command".to_string()];
        app.input = "my draft".to_string();
        app.cursor = app.input.len();
        app.input_up();
        assert_eq!(app.input, "old command");
        app.input_down();
        assert_eq!(app.input, "my draft", "черновик восстановлен, не потерян");
    }

    #[test]
    fn arrow_up_moves_cursor_in_multiline_not_history() {
        let mut app = editor_app();
        app.history = vec!["old".to_string()];
        app.input = "ab\ncde".to_string();
        app.cursor = app.input.len();
        app.input_up();
        assert_eq!(app.input, "ab\ncde", "ввод не заменён историей");
        assert!(app.history_index.is_none(), "в историю не входили");
        assert!(
            app.cursor <= 2,
            "курсор ушёл на первую строку: {}",
            app.cursor
        );
    }

    #[test]
    fn arrow_down_moves_cursor_in_multiline_not_history() {
        let mut app = editor_app();
        app.input = "abc\nde".to_string();
        app.cursor = 1;
        app.input_down();
        assert_eq!(app.input, "abc\nde");
        assert!(app.history_index.is_none());
        assert_eq!(app.cursor, 5, "курсор на второй строке, та же колонка");
    }
}
