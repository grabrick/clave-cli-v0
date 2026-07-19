use unicode_width::UnicodeWidthChar;

/// Ширина строки в терминальных колонках: CJK/эмодзи занимают 2, комбинирующие знаки — 0.
/// В отличие от `chars().count()`, совпадает с числом ячеек, которые рисует терминал, —
/// без этого курсор и перенос «съезжают» на широких символах. Суммируем посимвольно,
/// чтобы применялась эмодзи-поправка `char_display_width` (см. ниже).
pub(crate) fn display_width(text: &str) -> usize {
    text.chars().map(char_display_width).sum()
}

/// Ширина одного символа в колонках (0 для нулевой ширины/управляющих).
///
/// Поправка на эмодзи-презентацию: терминалы (Terminal.app, iTerm2 и прочие с эмодзи-
/// рендером) рисуют символы со свойством Emoji_Presentation в 2 клетки, тогда как
/// `unicode-width` относит часть из них — в том числе маркеры ленты `⏺` (ответ) и `⏹`
/// (остановка) — к ширине 1. Из-за расхождения inline-рендер терял счёт визуальных строк:
/// строка ответа переносилась там, где рендерер этого не ждал, высота блока «съезжала», и
/// при перерисовке строки дублировались или затирались. Приводим ширину к тому, что реально
/// рисует терминал.
pub(crate) fn char_display_width(ch: char) -> usize {
    if is_wide_emoji_presentation(ch) {
        return 2;
    }
    UnicodeWidthChar::width(ch).unwrap_or(0)
}

/// Символы с Emoji_Presentation, которые `unicode-width` считает узкими (1), а эмодзи-
/// терминалы рисуют широкими (2). Список — маркеры, которые clave сам ставит в ленту;
/// расширяется по мере находок обкатки, если модель выведет другой недооценённый эмодзи.
fn is_wide_emoji_presentation(ch: char) -> bool {
    matches!(ch, '⏺' | '⏹')
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_presentation_markers_measure_two_cells() {
        // ⏺ (ответ) и ⏹ (остановка) — Emoji_Presentation: терминал рисует их в 2 клетки,
        // unicode-width считает 1. Поправка обязана вернуть 2, иначе высота блока в
        // inline-рендере съезжает и строки дублируются/затираются (обкатка BUG-001/003).
        assert_eq!(char_display_width('⏺'), 2, "маркер ответа ⏺ — 2 клетки");
        assert_eq!(char_display_width('⏹'), 2, "маркер остановки ⏹ — 2 клетки");
        // Узкие маркеры и обычный текст — без изменений.
        assert_eq!(char_display_width('A'), 1);
        assert_eq!(char_display_width('➤'), 1, "➤ узкий и в терминале");
        assert_eq!(char_display_width('✻'), 1, "✻ узкий и в терминале");
        assert_eq!(char_display_width('я'), 1);
        // Поправка суммируется в display_width: "⏺ ok" = 2 + 1 + 1 + 1 = 5 (было 4).
        assert_eq!(
            display_width("⏺ ok"),
            5,
            "строка ответа шире на клетку маркера"
        );
        assert_eq!(
            display_width("обычный текст"),
            13,
            "кириллица без изменений"
        );
    }

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
}
