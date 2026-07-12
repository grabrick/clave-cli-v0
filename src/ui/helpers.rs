use super::*;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Ширина строки в терминальных колонках: CJK/эмодзи занимают 2, комбинирующие знаки — 0.
/// В отличие от `chars().count()`, совпадает с числом ячеек, которые рисует терминал, —
/// без этого курсор и перенос «съезжают» на широких символах.
pub(crate) fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Ширина одного символа в колонках (0 для нулевой ширины/управляющих).
pub(crate) fn char_display_width(ch: char) -> usize {
    UnicodeWidthChar::width(ch).unwrap_or(0)
}

/// Усечение по КОЛОНКАМ (в отличие от `truncate_chars`, который режет по символам).
/// Результат гарантированно занимает не больше `max_cols` колонок — иначе широкая
/// (CJK/эмодзи) строка пробила бы бюджет раскладки и упёрлась в правую границу.
pub(crate) fn truncate_display(text: &str, max_cols: usize) -> String {
    if display_width(text) <= max_cols {
        return text.to_string();
    }
    if max_cols == 0 {
        return String::new();
    }

    let mut out = String::new();
    let mut cols = 0usize;
    for ch in text.chars() {
        let w = char_display_width(ch);
        // Одну колонку держим под «…».
        if cols + w + 1 > max_cols {
            break;
        }
        out.push(ch);
        cols += w;
    }
    out.push('…');
    out
}

pub(crate) fn composer_height(app: &App, width: u16) -> u16 {
    let lines = input_lines_wrapped(&app.input, width).len() as u16;
    // +2 служебные строки: верхняя полоска (со встроенной плашкой) и нижняя полоска.
    (lines + 2).clamp(3, 10)
}

pub(crate) fn initial_transcript(_lang: Language) -> Vec<String> {
    Vec::new()
}

pub(crate) fn provider_count() -> usize {
    4
}

pub(crate) fn provider_mode(index: usize) -> Mode {
    match index {
        0 => Mode::CodexOnly,
        1 => Mode::ClaudeCodex,
        2 => Mode::CodexClaude,
        3 => Mode::ClaudeOnly,
        _ => Mode::CodexOnly,
    }
}

pub(crate) fn provider_index(mode: Mode) -> usize {
    match mode {
        Mode::CodexOnly => 0,
        Mode::ClaudeCodex => 1,
        Mode::CodexClaude => 2,
        Mode::ClaudeOnly => 3,
    }
}

pub(crate) fn provider_description(mode: Mode, lang: Language) -> &'static str {
    match mode {
        Mode::CodexOnly => lang.choose("Codex пишет и ревьюит", "Codex drafts and reviews"),
        Mode::ClaudeCodex => lang.choose(
            "Claude пишет, Codex ревьюит",
            "Claude drafts, Codex reviews",
        ),
        Mode::CodexClaude => lang.choose(
            "Codex пишет, Claude ревьюит",
            "Codex drafts, Claude reviews",
        ),
        Mode::ClaudeOnly => lang.choose("Claude пишет и ревьюит", "Claude drafts and reviews"),
    }
}

pub(crate) fn input_lines_wrapped(input: &str, width: u16) -> Vec<String> {
    let content_width = width.saturating_sub(2).max(1) as usize;
    if input.is_empty() {
        return vec![String::new()];
    }

    let mut rows = Vec::new();
    for line in input.split('\n') {
        rows.extend(wrap_terminal_text_preserving_spaces(line, content_width));
    }
    rows
}

pub(crate) fn input_cursor_position_wrapped(
    input: &str,
    cursor: usize,
    width: u16,
) -> (usize, usize) {
    let content_width = width.saturating_sub(2).max(1) as usize;
    let mut visual_line = 0usize;
    let mut visual_col = 0usize;
    let mut has_content = false;

    // Идём тем же обходом, что и wrap_terminal_text_preserving_spaces, чтобы позиция
    // курсора и перенос строк ВСЕГДА совпадали (иначе курсор «съезжает» на широких
    // символах). Ширину считаем в колонках, а не в символах.
    for ch in input[..cursor].chars() {
        if ch == '\n' {
            visual_line += 1;
            visual_col = 0;
            has_content = false;
            continue;
        }
        let w = char_display_width(ch);
        if visual_col + w > content_width && has_content {
            visual_line += 1;
            visual_col = 0;
        }
        visual_col += w;
        has_content = true;
    }

    (visual_line, visual_col)
}

pub(crate) fn wrap_terminal_line(text: &str, width: u16) -> Vec<String> {
    let max_chars = width.saturating_sub(1).max(1) as usize;
    wrap_terminal_text_preserving_spaces(text, max_chars)
}

pub(crate) fn wrap_terminal_text_preserving_spaces(text: &str, max_cols: usize) -> Vec<String> {
    let max_cols = max_cols.max(1);
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut rows = Vec::new();
    let mut current = String::new();
    // Ширину ведём инкрементально в КОЛОНКАХ (CJK/эмодзи = 2): и ради O(n) на кадр, и
    // чтобы перенос совпадал с раскладкой терминала и с input_cursor_position_wrapped.
    let mut current_cols = 0usize;

    for ch in text.chars() {
        if ch == '\n' {
            rows.push(std::mem::take(&mut current));
            current_cols = 0;
            continue;
        }

        let w = char_display_width(ch);
        // Широкий символ не влезает в остаток строки — переносим ПЕРЕД ним.
        if current_cols + w > max_cols && !current.is_empty() {
            rows.push(std::mem::take(&mut current));
            current_cols = 0;
        }
        current.push(ch);
        current_cols += w;
    }

    rows.push(current);
    rows
}

pub(crate) fn wrap_chars(text: &str, max_chars: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let max_chars = max_chars.max(1);
    let mut rows = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        let current_len = display_width(&current);
        let word_len = display_width(word);
        let extra_space = usize::from(!current.is_empty());

        if current_len + extra_space + word_len > max_chars && !current.is_empty() {
            rows.push(current);
            current = String::new();
        }

        if word_len > max_chars {
            if !current.is_empty() {
                rows.push(current);
                current = String::new();
            }

            let mut chunk = String::new();
            for ch in word.chars() {
                if display_width(&chunk) >= max_chars {
                    rows.push(chunk);
                    chunk = String::new();
                }
                chunk.push(ch);
            }
            if !chunk.is_empty() {
                current = chunk;
            }
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
    }

    if !current.is_empty() {
        rows.push(current);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_preserving_spaces_is_stable() {
        // Характеризующий тест: оптимизация (O(n)) обязана давать тот же результат.
        assert_eq!(wrap_terminal_text_preserving_spaces("", 5), vec![""]);
        assert_eq!(wrap_terminal_text_preserving_spaces("abc", 5), vec!["abc"]);
        assert_eq!(
            wrap_terminal_text_preserving_spaces("abcde", 5),
            vec!["abcde"]
        );
        assert_eq!(
            wrap_terminal_text_preserving_spaces("abcdef", 5),
            vec!["abcde", "f"]
        );
        assert_eq!(
            wrap_terminal_text_preserving_spaces("ab\ncd", 5),
            vec!["ab", "cd"]
        );
        // Юникод считается по символам, а не байтам.
        assert_eq!(
            wrap_terminal_text_preserving_spaces("абвгде", 5),
            vec!["абвгд", "е"]
        );
    }

    #[test]
    fn wrap_chars_keeps_words_whole() {
        // Слово не рвётся посреди буквы: «world» уезжает на новую строку целиком.
        assert_eq!(wrap_chars("hello world", 7), vec!["hello", "world"]);
        // Слово длиннее ширины влезть целиком не может — дробится по символам.
        assert_eq!(wrap_chars("abcdefgh", 5), vec!["abcde", "fgh"]);
        // Путь со спецсимволами короче ширины — переносится целиком, не по буквам.
        assert_eq!(
            wrap_chars("see src/app.rs now", 10),
            vec!["see", "src/app.rs", "now"]
        );
    }

    #[test]
    fn wrap_and_cursor_agree_on_wide_chars() {
        // Широкий символ (あ = 2 колонки) переносится по КОЛОНКАМ, а не по символам.
        assert_eq!(
            wrap_terminal_text_preserving_spaces("aああ", 4),
            vec!["aあ", "あ"]
        );
        // Перенос и позиция курсора идут одним обходом: строка курсора == число строк − 1,
        // а колонка == ширине последней строки в колонках.
        let input = "aあb\ncd";
        let rows = input_lines_wrapped(input, 10); // content_width = 8
        assert_eq!(rows, vec!["aあb", "cd"]);
        let (line, col) = input_cursor_position_wrapped(input, input.len(), 10);
        assert_eq!((line, col), (rows.len() - 1, display_width("cd")));
    }
}
