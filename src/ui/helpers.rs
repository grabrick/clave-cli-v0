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
    if rows.is_empty() {
        // Непустой вход (например, строка из одних пробелов) не дал ни одной строки:
        // возвращаем пустую строку, чтобы строка-разделитель сохранила вертикальное место.
        rows.push(String::new());
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_display_cuts_by_columns_and_keeps_the_ellipsis_inside() {
        // Ровно по бюджету — строка цела, «…» не появляется.
        assert_eq!(truncate_display("abc", 3), "abc");
        // На колонку шире — режем так, чтобы «…» ВЛЕЗЛО в бюджет: 4 символа + «…» = 5.
        assert_eq!(truncate_display("abcdef", 5), "abcd…");
        // Широкие (CJK) символы считаются за 2 колонки: 2 + 2 + «…» = 5.
        assert_eq!(truncate_display("機能機能", 5), "機能…");
        // Нулевой бюджет — пусто, даже без «…».
        assert_eq!(truncate_display("abc", 0), "");
    }

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

    // ── wrap_chars: перенос по словам и разбивка длинных слов ────────────────

    /// Короткие слова склеиваются через пробел, а перенос идёт РОВНО перед словом,
    /// что не влезло. Границу берём впритык, чтобы `>`/`>=` и `+`/`-`/`*` разошлись.
    #[test]
    fn wrap_chars_breaks_exactly_before_the_overflowing_word() {
        // 3 + 1(пробел) + 3 = 7 > 6 → «bbb» уезжает на новую строку.
        // Ловит: delete `!` (185), `+`→`-`/`*` (187:24) и `+`→`*` (187:38) —
        // при любой из подмен сумма падает до ≤6 и перенос не срабатывает.
        assert_eq!(wrap_chars("aaa bbb", 6), vec!["aaa", "bbb"]);
        // 3 + 1 + 2 = 6, РОВНО ширина → переносить нечего, слова остаются вместе.
        // Здесь `>` и `>=` (187:49) расходятся: `>=` порвал бы строку зря.
        assert_eq!(wrap_chars("aaa bb", 6), vec!["aaa bb"]);
    }

    /// Слово длиннее ширины целиком влезть не может — дробится ровно по max_chars,
    /// последний кусок короче. Точные куски, а не длина.
    #[test]
    fn wrap_chars_splits_a_word_longer_than_the_width_into_exact_chunks() {
        assert_eq!(wrap_chars("aaaaaaaaaa", 4), vec!["aaaa", "aaaa", "aa"]);
    }

    /// Слово из буквы и комбинирующего знака (нулевой ширины) при max_chars=1 занимает
    /// РОВНО одну колонку и рвать его нельзя. Здесь `>` и `>=` (192:21) расходятся: `>=`
    /// вошёл бы в дробление и оторвал бы диакритику от буквы.
    #[test]
    fn wrap_chars_keeps_a_combining_mark_attached_to_its_base() {
        assert_eq!(wrap_chars("e\u{0301}", 1), vec!["e\u{0301}"]);
    }

    /// Строка из одних пробелов не должна ИСЧЕЗАТЬ: непустой вход обязан дать хотя бы
    /// одну (пустую) строку, иначе строка-разделитель теряет вертикальное место в ленте.
    #[test]
    fn wrap_chars_keeps_a_whitespace_only_line_as_one_empty_row() {
        assert_eq!(wrap_chars("   ", 6), vec![""]);
    }

    // ── input_cursor_position_wrapped: (визуальная строка, колонка) ──────────

    /// Курсор идёт тем же обходом, что и перенос: за точкой переноса он переезжает
    /// на следующую визуальную строку. Узкая ширина, чтобы граница была видна.
    #[test]
    fn cursor_follows_the_wrap_exactly() {
        // width=5 → content_width=3. «abcdef» рвётся после 3-й колонки: «abc»/«def».
        // Курсор в конце — строка 1, колонка 3. Один этот assert валит все пять
        // мутантов 124–128: `==`/`>=` дают (2,2), `*` (124:23) — (1,2),
        // `-=` — underflow-паника, `*=` — (0,0).
        assert_eq!(input_cursor_position_wrapped("abcdef", 6, 5), (1, 3));
        // Курсор в середине первой строки — строка 0, колонка 2.
        assert_eq!(input_cursor_position_wrapped("abcdef", 2, 5), (0, 2));
        // Широкий символ (2 колонки) не разрывается: у границы переносится ПЕРЕД ним.
        assert_eq!(
            input_cursor_position_wrapped("ab機", "ab機".len(), 5),
            (1, 2)
        );
        // Явный \n: курсор на следующей строке, колонка 0.
        assert_eq!(input_cursor_position_wrapped("ab\ncd", 3, 10), (1, 0));
    }

    // ── Провайдеры: индекс ⇄ режим ⇄ описание ────────────────────────────────

    /// Каждый режим — свой слот. Четыре разных числа валят `->0` и `->1` (67:5).
    #[test]
    fn provider_index_maps_each_mode_to_its_own_slot() {
        assert_eq!(provider_index(Mode::CodexOnly), 0);
        assert_eq!(provider_index(Mode::ClaudeCodex), 1);
        assert_eq!(provider_index(Mode::CodexClaude), 2);
        assert_eq!(provider_index(Mode::ClaudeOnly), 3);
    }

    /// Обратное отображение + круговой проход. `provider_mode(2)` == `CodexClaude`
    /// (и round-trip) валит delete match arm 2 (60): без арма 2 индекс 2 съехал бы
    /// в дефолт `CodexOnly` и round-trip бы сломался.
    #[test]
    fn provider_mode_is_the_inverse_of_provider_index() {
        assert_eq!(provider_mode(0), Mode::CodexOnly);
        assert_eq!(provider_mode(1), Mode::ClaudeCodex);
        assert_eq!(provider_mode(2), Mode::CodexClaude);
        assert_eq!(provider_mode(3), Mode::ClaudeOnly);
        // Вне диапазона — откат к первому режиму.
        assert_eq!(provider_mode(4), Mode::CodexOnly);
        assert_eq!(provider_mode(usize::MAX), Mode::CodexOnly);
        for index in 0..provider_count() {
            assert_eq!(provider_index(provider_mode(index)), index);
        }
    }

    /// У каждого режима своё непустое описание на обоих языках. Непустота валит `->""`,
    /// различность — `->"xyzzy"` (76:5, при нём все четыре стали бы одинаковы).
    #[test]
    fn provider_description_is_non_empty_and_distinct_per_mode() {
        let modes = [
            Mode::CodexOnly,
            Mode::ClaudeCodex,
            Mode::CodexClaude,
            Mode::ClaudeOnly,
        ];
        for lang in [Language::Ru, Language::En] {
            let texts: Vec<&str> = modes
                .iter()
                .map(|&mode| provider_description(mode, lang))
                .collect();
            for text in &texts {
                assert!(!text.is_empty(), "описание режима пустое на {lang:?}");
            }
            let unique: std::collections::HashSet<&str> = texts.iter().copied().collect();
            assert_eq!(
                unique.len(),
                modes.len(),
                "описания режимов обязаны различаться ({lang:?})"
            );
        }
        // Точные строки, чтобы «xyzzy» и перепутанные режимы не прошли.
        assert_eq!(
            provider_description(Mode::CodexOnly, Language::Ru),
            "Codex пишет и ревьюит"
        );
        assert_eq!(
            provider_description(Mode::ClaudeOnly, Language::En),
            "Claude drafts and reviews"
        );
    }

    // ── composer_height: (строки ввода + 2), зажатое в [3, 10] ────────────────

    fn composer_app(input: &str) -> App {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = std::env::temp_dir().join(format!(
            "clave-helpers-{}-{}",
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
        app.input = input.to_string();
        app
    }

    /// Высота = строки ввода + 2 служебные, зажатые в [3, 10].
    #[test]
    fn composer_height_is_input_lines_plus_two_clamped_to_three_and_ten() {
        // Пустой ввод: 1 + 2 = 3, нижний кламп. Валит `->0` и `->1` (43:5).
        assert_eq!(composer_height(&composer_app(""), 20), 3);
        // Три строки: 3 + 2 = 5. Здесь `+` и `*` (45:12) расходятся: 3*2=6 ≠ 5.
        assert_eq!(composer_height(&composer_app("a\nb\nc"), 20), 5);
        // Пятнадцать строк: 15 + 2 = 17, верхний кламп режет до 10.
        let tall = "x\n".repeat(15);
        assert_eq!(composer_height(&composer_app(&tall), 20), 10);
    }
}
