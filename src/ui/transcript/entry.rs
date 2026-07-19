use super::*;

/// Стилизует и переносит одну строку истории в готовые `Line` для `insert_before`.
/// `state` ведёт code-block между строками (история append-only — state монотонен).
pub(crate) fn history_line_render(
    line: &str,
    lang: Language,
    width: u16,
    theme: Theme,
    state: &mut TranscriptRenderState,
) -> Vec<Line<'static>> {
    transcript_entry_lines_with_state(line, lang, width, theme, state)
}

#[derive(Default, Clone, Copy)]
pub(crate) struct TranscriptRenderState {
    in_code_block: bool,
}

pub(crate) fn transcript_entry_lines_with_state(
    line: &str,
    lang: Language,
    width: u16,
    theme: Theme,
    state: &mut TranscriptRenderState,
) -> Vec<Line<'static>> {
    if let Some(message) = line.strip_prefix("◆ ") {
        state.in_code_block = false;
        // Пустая строка перед репликой пользователя — отделяет ход от предыдущего.
        let mut out = vec![Line::from("")];
        out.extend(user_message_lines(message, width, theme));
        return out;
    }

    if is_markdown_fence(line) {
        state.in_code_block = !state.in_code_block;
        return Vec::new();
    }

    if state.in_code_block {
        return code_block_lines(line, width, theme);
    }

    // Welcome-строки (логотип + инфо) рендерим БЕЗ переноса: логотип чувствителен к
    // пробелам, а wrap_chars (перенос по словам) их схлопнул бы.
    if line.starts_with(WELCOME_NAME)
        || line.starts_with(WELCOME_INFO)
        || line.starts_with(WELCOME_HINT)
    {
        return vec![style_transcript_line(line, lang, theme)];
    }

    // Воздух перед началом ответа (⏺) и эхо команды (❯), чтобы реплики не слипались.
    let mut out = Vec::new();
    if line.starts_with("⏺ ") || line.starts_with("❯ ") {
        out.push(Line::from(""));
    }
    // Проза переносится ПО СЛОВАМ (wrap_chars), а не по символам — иначе слова,
    // особенно со спецсимволами (пути, URL), рвутся посреди буквы. Ввод и code-блоки
    // остаются на посимвольном wrap (там важны курсор-математика и сохранение пробелов).
    let max_chars = width.saturating_sub(1).max(1) as usize;
    out.extend(
        wrap_chars(line, max_chars)
            .into_iter()
            .map(|wrapped| style_transcript_line(&wrapped, lang, theme)),
    );
    out
}

/// Реплика пользователя: стрелка-маркер + текст на залитом фоном «пузыре».
/// Без рамки и подписи «Ты» — отправленное сообщение видно по фону.
pub(crate) fn user_message_lines(message: &str, width: u16, theme: Theme) -> Vec<Line<'static>> {
    let arrow_style = Style::default()
        .fg(theme.accent())
        .add_modifier(Modifier::BOLD);
    let bubble_style = Style::default()
        .fg(Color::White)
        .bg(theme.accent_bg())
        .add_modifier(Modifier::BOLD);

    // «➤ » (2 ячейки) + по пробелу-полю слева/справа внутри пузыря = 4 ячейки.
    let content_width = (width as usize).saturating_sub(4).max(8);
    let wrapped = wrap_chars(message, content_width);
    // Пузырь обнимает текст: ширина = самая длинная строка (не на всю ширину экрана).
    let bubble = wrapped
        .iter()
        .map(|line| display_width(line))
        .max()
        .unwrap_or(0);

    wrapped
        .iter()
        .enumerate()
        .map(|(index, line)| {
            // Стрелка только на первой строке, продолжения — отступ под текст.
            let prefix = if index == 0 { "➤ " } else { "  " };
            let pad = " ".repeat(bubble.saturating_sub(display_width(line)));
            Line::from(vec![
                Span::styled(prefix, arrow_style),
                Span::styled(format!(" {line}{pad} "), bubble_style),
            ])
        })
        .collect()
}

pub(crate) fn is_markdown_fence(line: &str) -> bool {
    let trimmed = line.trim_start();
    let without_status = trimmed
        .strip_prefix("⏺ ")
        .map(str::trim_start)
        .unwrap_or(trimmed);

    without_status.starts_with("```") || without_status.starts_with("~~~")
}

pub(crate) fn code_block_lines(line: &str, width: u16, theme: Theme) -> Vec<Line<'static>> {
    let content_width = width.saturating_sub(3).max(1);
    wrap_terminal_line(line, content_width)
        .into_iter()
        .map(|wrapped| {
            Line::from(vec![
                Span::styled("  ", Style::default().fg(theme.accent_dim())),
                Span::styled(wrapped, Style::default().fg(Color::Gray)),
            ])
        })
        .collect()
}

pub(crate) fn separator_line(width: u16, theme: Theme) -> Line<'static> {
    Line::styled(
        "─".repeat(width as usize),
        Style::default().fg(theme.accent_dim()),
    )
}

// ── Кликабельные пути (OSC 8) ────────────────────────────────────────────────
//
// Детекция путей и навешивание гиперссылок идут ОТДЕЛЬНЫМ пост-проходом
// (`attach_links`) поверх уже отрисованной строки — стилизация не меняется.
// URL строит сам clave (`open_url`), из контента он не берётся; печать
// (`render::queue_rich_line`) санитайзит текст по-прежнему. Линки нужны только в
// истории (скроллбэк), поэтому живой блок остаётся на `Vec<Line>`.

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn user_message_uses_arrow_and_background_not_box() {
        let lines = user_message_lines("привет мир", 80, Theme::Purple);
        assert_eq!(lines.len(), 1, "короткое сообщение — одна строка");
        let text: String = plain(&lines[0]);
        // Стрелка-маркер есть, рамки и подписи «Ты» — нет.
        assert!(text.starts_with("➤ "), "ведущая стрелка: {text:?}");
        assert!(!text.contains("Ты") && !text.contains("You"));
        for ch in ['╭', '╮', '╰', '╯', '│', '─'] {
            assert!(!text.contains(ch), "нет символов рамки: {ch}");
        }
        // Текст лежит на залитом фоном «пузыре» (bg = accent_bg темы).
        let bubble = lines[0]
            .spans
            .iter()
            .find(|s| s.content.contains("привет мир"))
            .expect("есть спан с текстом");
        assert_eq!(
            bubble.style.bg,
            Some(Theme::Purple.accent_bg()),
            "фон-пузырь"
        );

        // Многострочное сообщение: стрелка только на первой строке.
        let many = user_message_lines(&"слово ".repeat(60), 40, Theme::Purple);
        assert!(many.len() > 1);
        assert!(plain(&many[0]).starts_with("➤ "));
        assert!(
            plain(&many[1]).starts_with("  "),
            "продолжение — отступ, без стрелки"
        );
    }

    #[test]
    fn hides_markdown_code_fence_markers() {
        let transcript = [
            "⏺ Вот пример:".to_string(),
            "```text".to_string(),
            "Покажи текущее состояние проекта".to_string(),
            "```".to_string(),
            "Готово.".to_string(),
        ];
        let mut state = TranscriptRenderState::default();
        let rendered = transcript
            .iter()
            .flat_map(|line| {
                transcript_entry_lines_with_state(line, Language::Ru, 80, Theme::Purple, &mut state)
            })
            .map(|line| plain(&line))
            .collect::<Vec<_>>();

        assert!(!rendered.iter().any(|line| line.contains("```")));
        assert!(rendered
            .iter()
            .any(|line| line.contains("Покажи текущее состояние проекта")));
        assert!(rendered.iter().any(|line| line.contains("Готово.")));
    }

    #[test]
    fn code_block_state_persists_across_lines() {
        // История append-only: один `state` ведёт fence между вызовами
        // `history_line_render`. Внутри fence строки — как код, после — обычные.
        let lines = ["```rust", "let x = 1;", "```", "обычный текст"];
        let mut state = TranscriptRenderState::default();
        let rendered = lines
            .iter()
            .flat_map(|line| history_line_render(line, Language::Ru, 80, Theme::Purple, &mut state))
            .collect::<Vec<_>>();

        // Маркеры fence сами по себе не дают строк.
        assert!(!rendered.iter().any(|l| plain(l).contains("```")));

        // Строка внутри fence отрисована как код: серое содержимое и отступ.
        let code = rendered
            .iter()
            .find(|l| plain(l).contains("let x = 1;"))
            .expect("строка кода отрисована");
        assert!(plain(code).starts_with("  "), "код имеет отступ");
        assert!(
            code.spans.iter().any(|s| s.style.fg == Some(Color::Gray)),
            "содержимое кода — серым"
        );

        // Строка после закрывающего fence — обычная, без серой подсветки кода.
        let normal = rendered
            .iter()
            .find(|l| plain(l).contains("обычный текст"))
            .expect("обычная строка отрисована");
        assert!(
            normal.spans.iter().all(|s| s.style.fg != Some(Color::Gray)),
            "после fence подсветка кода снята"
        );

        // state вернулся в обычный режим.
        assert!(!state.in_code_block, "fence закрыт — state сброшен");
    }

    #[test]
    fn separator_line_follows_active_theme() {
        // Разделитель должен брать цвет из активной темы, а не из захардкоженной палитры.
        for theme in [
            Theme::Purple,
            Theme::Cyan,
            Theme::Rose,
            Theme::Amber,
            Theme::Mono,
        ] {
            assert_eq!(separator_line(12, theme).style.fg, Some(theme.accent_dim()));
        }
        // Регрессия на «вечно фиолетовый»: смена темы должна менять цвет разделителя.
        assert_ne!(
            separator_line(12, Theme::Cyan).style.fg,
            separator_line(12, Theme::Purple).style.fg,
        );
    }

    // ── transcript_entry_lines_with_state ────────────────────────────────────
    #[test]
    fn welcome_lines_are_not_word_wrapped() {
        // Длинный контент при узкой ширине: welcome-ветка держит одну строку, а
        // провал в wrap_chars (мутанты 47/48) порвал бы её на несколько.
        for sentinel in [WELCOME_NAME, WELCOME_INFO, WELCOME_HINT] {
            let line = format!("{sentinel}one two three four five six");
            let mut state = TranscriptRenderState::default();
            let out = transcript_entry_lines_with_state(
                &line,
                Language::Ru,
                10,
                Theme::Purple,
                &mut state,
            );
            assert_eq!(out.len(), 1, "welcome не переносится ({sentinel:?})");
        }
    }

    #[test]
    fn answer_and_command_get_leading_air_line() {
        // «⏺ » и «❯ » дают пустую строку-воздух первой; 55 ||→&& уберёт её.
        for (text, marker) in [("⏺ ответ бота", "ответ"), ("❯ команда", "команда")]
        {
            let mut state = TranscriptRenderState::default();
            let out = transcript_entry_lines_with_state(
                text,
                Language::Ru,
                80,
                Theme::Purple,
                &mut state,
            );
            assert!(out.len() >= 2, "{text}: воздух + строка");
            assert!(plain(&out[0]).is_empty(), "{text}: первая строка — воздух");
            assert!(plain(&out[1]).contains(marker), "{text}: содержимое ниже");
        }
    }
}
