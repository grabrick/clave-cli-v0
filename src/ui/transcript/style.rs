use super::*;

/// Разбивает строку на спаны, подсвечивая inline-код в обратных кавычках.
/// Незакрытые кавычки оставляются как есть (перенос строки мог разорвать пару).
fn inline_code_spans(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close_rel) = after.find('`') else {
            break;
        };
        if open > 0 {
            spans.push(Span::raw(rest[..open].to_string()));
        }
        spans.push(Span::styled(
            after[..close_rel].to_string(),
            Style::default().fg(Color::Indexed(180)),
        ));
        rest = &after[close_rel + 1..];
    }
    if !rest.is_empty() {
        spans.push(Span::raw(rest.to_string()));
    }
    if spans.is_empty() {
        spans.push(Span::raw(text.to_string()));
    }
    spans
}

/// Разбивает строку на спаны, подсвечивая inline-код (`код`) и **жирный** текст.
/// Маркеры удаляются — остаётся только стиль. Inline-код имеет приоритет над
/// жирным, поэтому `**` внутри кода трактуется буквально. Незакрытые маркеры
/// остаются обычным текстом (перенос строки мог разорвать пару).
fn inline_md_spans(text: &str) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Inline-код в обратных кавычках.
        if bytes[i] == b'`' {
            if let Some(close) = text[i + 1..].find('`') {
                if !buf.is_empty() {
                    spans.push(Span::raw(std::mem::take(&mut buf)));
                }
                spans.push(Span::styled(
                    text[i + 1..i + 1 + close].to_string(),
                    Style::default().fg(Color::Indexed(180)),
                ));
                i += 1 + close + 1;
                continue;
            }
        }
        // **Жирный** текст (с возможным inline-кодом внутри).
        if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'*') {
            if let Some(close) = text[i + 2..].find("**") {
                let inner = &text[i + 2..i + 2 + close];
                if !inner.is_empty() {
                    if !buf.is_empty() {
                        spans.push(Span::raw(std::mem::take(&mut buf)));
                    }
                    for mut span in inline_code_spans(inner) {
                        span.style = span.style.add_modifier(Modifier::BOLD);
                        spans.push(span);
                    }
                    i += 2 + close + 2;
                    continue;
                }
            }
        }
        // *Курсив* — одиночная звёздочка (двойную `**` уже разобрали выше). Содержимое
        // должно прилегать к непробельным символам, иначе это оператор (`2 * 3`), а не
        // разметка; внутренних звёздочек не допускаем, чтобы не проглотить соседний акцент.
        if bytes[i] == b'*' {
            if let Some(close) = text[i + 1..].find('*') {
                let inner = &text[i + 1..i + 1 + close];
                if !inner.is_empty()
                    && !inner.contains('*')
                    && !inner.starts_with(char::is_whitespace)
                    && !inner.ends_with(char::is_whitespace)
                {
                    if !buf.is_empty() {
                        spans.push(Span::raw(std::mem::take(&mut buf)));
                    }
                    for mut span in inline_code_spans(inner) {
                        span.style = span.style.add_modifier(Modifier::ITALIC);
                        spans.push(span);
                    }
                    i += 1 + close + 1;
                    continue;
                }
            }
        }
        // Обычный символ — копим в буфер, шагаем по границе UTF-8.
        let ch = text[i..].chars().next().unwrap();
        buf.push(ch);
        i += ch.len_utf8();
    }
    if !buf.is_empty() {
        spans.push(Span::raw(buf));
    }
    if spans.is_empty() {
        spans.push(Span::raw(text.to_string()));
    }
    spans
}

pub(crate) fn style_transcript_line(line: &str, lang: Language, theme: Theme) -> Line<'static> {
    if line.starts_with("◆ ") {
        Line::from(vec![
            Span::styled(
                lang.choose("Ты", "You"),
                Style::default()
                    .fg(theme.accent())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::raw(line.trim_start_matches("◆ ").to_string()),
        ])
    } else if let Some(command) = line.strip_prefix("❯ ") {
        Line::from(vec![
            Span::styled(
                "❯ ",
                Style::default()
                    .fg(theme.accent())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(command.to_string()),
        ])
    } else if line.starts_with("Final brief: ") {
        Line::from(vec![
            Span::styled(
                "⏺ brief ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(line.trim_start_matches("Final brief: ").to_string()),
        ])
    } else if is_error_status_line(line) {
        Line::styled(line.to_string(), Style::default().fg(Color::Red))
    } else if line.starts_with("Drafting")
        || line.starts_with("Review")
        || line.starts_with("Revision")
    {
        Line::from(vec![
            Span::styled(
                "⏺ ",
                Style::default()
                    .fg(theme.accent())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(line.to_string()),
        ])
    } else if line.starts_with("⎿ ") || line.trim_start().starts_with('⎿') {
        Line::styled(line.to_string(), Style::default().fg(Color::DarkGray))
    } else if let Some(rest) = line.strip_prefix("⏺ ") {
        let mut spans = vec![Span::styled(
            "⏺ ",
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD),
        )];
        spans.extend(inline_md_spans(rest));
        Line::from(spans)
    } else if line.starts_with("✻ ") || line.starts_with("✦ ") {
        Line::styled(
            line.to_string(),
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD),
        )
    } else if line.starts_with("🅐 ") {
        // Заголовок шага исполнителя в тандеме — цветом акцента.
        Line::styled(
            line.to_string(),
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD),
        )
    } else if line.starts_with("🅒 ") {
        // Заголовок шага критика в тандеме — отдельным цветом (как режим Tandem).
        Line::styled(
            line.to_string(),
            Style::default()
                .fg(Color::Indexed(170))
                .add_modifier(Modifier::BOLD),
        )
    } else if let Some(heading) = line.strip_prefix("### ") {
        Line::styled(
            format!("  {heading}"),
            Style::default()
                .fg(theme.accent_soft())
                .add_modifier(Modifier::BOLD),
        )
    } else if let Some(heading) = line.strip_prefix("## ") {
        Line::styled(
            heading.to_string(),
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD),
        )
    } else if let Some(heading) = line.strip_prefix("# ") {
        Line::styled(
            heading.to_string(),
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )
    } else if let Some(item) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
        let mut spans = vec![Span::styled("• ", Style::default().fg(theme.accent()))];
        spans.extend(inline_md_spans(item));
        Line::from(spans)
    } else if let Some(quote) = line.strip_prefix("> ") {
        Line::styled(
            format!("▏ {quote}"),
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::ITALIC),
        )
    } else if let Some(rest) = line.strip_prefix(WELCOME_NAME) {
        welcome_name_line(rest)
    } else if let Some(rest) = line.strip_prefix(WELCOME_INFO) {
        welcome_info_line(rest)
    } else if let Some(rest) = line.strip_prefix(WELCOME_HINT) {
        Line::styled(format!("  {rest}"), Style::default().fg(Color::Gray))
    } else {
        Line::from(inline_md_spans(line))
    }
}

pub(crate) fn is_error_status_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    let lower = trimmed.to_ascii_lowercase();

    lower.starts_with("error:")
        || lower.starts_with("failed:")
        || lower.starts_with("failed ")
        || lower.starts_with("wait failed:")
        || lower.starts_with("engine missing")
        || lower.contains("returned an error")
        || lower.contains("failed to spawn")
        || lower.contains("завершился с кодом")
        || lower.contains("вернул ошибку")
        || (trimmed.starts_with("⎿ ")
            && (lower.contains("error")
                || lower.contains("failed")
                || lower.contains("read-only file system")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_md_renders_single_asterisk_italic() {
        // *курсив* (одиночная звёздочка) — italic-спан без звёздочек (обкатка BUG-002 markdown).
        let spans = inline_md_spans("тут *важное* слово");
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(
            text, "тут важное слово",
            "звёздочки курсива должны исчезнуть"
        );
        assert!(
            spans.iter().any(|s| s.content.as_ref() == "важное"
                && s.style.add_modifier.contains(Modifier::ITALIC)),
            "«важное» обязано стать курсивом"
        );
        // Скобка-обёртка как в обкатке: *(Замечу)* → курсив без звёздочек.
        let paren: String = inline_md_spans("*(Замечу)* далее")
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(paren, "(Замечу) далее");
        // Оператор умножения с пробелами — НЕ разметка, звёздочки остаются.
        let mult: String = inline_md_spans("2 * 3 * 4")
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(mult, "2 * 3 * 4", "оператор умножения не курсив");
        // **жирный** и `код` по-прежнему работают.
        let bold: String = inline_md_spans("**жир** и `код`")
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(bold, "жир и код");
    }

    #[test]
    fn does_not_treat_plain_error_words_as_status_errors() {
        assert!(!is_error_status_line(
            "- слово error внутри обычного ответа не должно красить строку"
        ));
        assert!(is_error_status_line("Failed to spawn codex"));
        assert!(is_error_status_line("⎿ Read-only file system"));
    }

    #[test]
    fn inline_code_splits_backticks() {
        let spans = inline_code_spans("use `cargo build` now");
        let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "use cargo build now");
        assert!(spans.len() >= 2);
        // незакрытая кавычка не ломает рендер
        let one = inline_code_spans("broken ` tail");
        let joined: String = one.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "broken ` tail");
    }

    #[test]
    fn inline_bold_strips_markers_and_styles() {
        // **жирный** → спан с модификатором BOLD без звёздочек.
        let spans = inline_md_spans("есть **важное** слово");
        let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "есть важное слово");
        assert!(
            spans
                .iter()
                .any(|s| s.content == "важное" && s.style.add_modifier.contains(Modifier::BOLD)),
            "жирный фрагмент несёт модификатор BOLD"
        );
        // Маркеры `**` не должны просочиться ни в один спан.
        assert!(spans.iter().all(|s| !s.content.contains('*')));

        // Регрессия из реального бага: нумерованный пункт «1. **Память:** …»
        // идёт в общую ветку и должен потерять звёздочки, но не цифры.
        let line = style_transcript_line("1. **Память:** важно", Language::Ru, Theme::Purple);
        let joined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "1. Память: важно");
        assert!(line
            .spans
            .iter()
            .any(|s| s.content == "Память:" && s.style.add_modifier.contains(Modifier::BOLD)));

        // Незакрытый `**` остаётся буквальным и не съедает последующий inline-код.
        let spans = inline_md_spans("a ** b `c`");
        let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "a ** b c");
        assert!(spans
            .iter()
            .any(|s| s.content == "c" && s.style.fg == Some(Color::Indexed(180))));
    }

    // ── is_error_status_line: по маркеру на строку (все ||→&&) ────────────────
    #[test]
    fn error_status_line_catches_each_marker() {
        // Каждая строка несёт РОВНО один маркер: подмена соседнего ||→&& требует
        // двух истинных условий разом и обнуляет результат — тест краснеет.
        for text in [
            "error: boom",
            "failed: boom",
            "failed to compile", // маркер "failed "
            "wait failed: x",
            "engine missing x",
            "boom returned an error",
            "boom failed to spawn",
            "процесс завершился с кодом 1",
            "codex вернул ошибку",
            "⎿ read-only file system",
            "⎿ compile error", // бьёт 392: только error, без failed/read-only
            "⎿ deploy failed", // бьёт 392: только failed, без error/read-only
        ] {
            assert!(is_error_status_line(text), "маркер ошибки: {text:?}");
        }
        // Контроль от мутантов «всегда true».
        assert!(!is_error_status_line("всё хорошо, готово"));
    }

    // ── style_transcript_line ────────────────────────────────────────────────
    #[test]
    fn style_step_headers_get_accent_prefix() {
        // Три ветки Drafting/Review/Revision: ||→&& (224/225) убирает две из трёх.
        let accent = Theme::Purple.accent();
        for text in ["Drafting the plan", "Review pass", "Revision cycle"] {
            let line = style_transcript_line(text, Language::Ru, Theme::Purple);
            assert_eq!(line.spans[0].content.as_ref(), "⏺ ", "{text}: префикс-спан");
            assert_eq!(line.spans[0].style.fg, Some(accent), "{text}: цвет акцента");
            assert!(
                line.spans[0].style.add_modifier.contains(Modifier::BOLD),
                "{text}: жирный префикс"
            );
            let joined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(joined.contains(text), "{text}: текст на месте");
        }
    }

    #[test]
    fn style_continuation_line_is_dark_gray_even_with_indent() {
        // «⎿ x» и «  ⎿ x» (с пробелами) — обе DarkGray; вторая бьёт 236 ||→&&.
        for text in ["⎿ x", "  ⎿ x"] {
            let line = style_transcript_line(text, Language::Ru, Theme::Purple);
            assert_eq!(line.style.fg, Some(Color::DarkGray), "{text:?}");
        }
    }

    #[test]
    fn style_thinking_markers_are_bold_accent() {
        // «✻» и «✦» — обе акцент+BOLD; 247 ||→&& уронит любую из них в else.
        let accent = Theme::Purple.accent();
        for text in ["✻ думаю", "✦ думаю"] {
            let line = style_transcript_line(text, Language::Ru, Theme::Purple);
            assert_eq!(line.style.fg, Some(accent), "{text:?}");
            assert!(line.style.add_modifier.contains(Modifier::BOLD), "{text:?}");
        }
    }

    #[test]
    fn style_h1_is_bold_and_underlined() {
        // «# …» несёт ОБА модификатора; 289 |→& даёт пустой набор.
        let line = style_transcript_line("# Заголовок", Language::Ru, Theme::Purple);
        assert!(
            line.style.add_modifier.contains(Modifier::BOLD),
            "BOLD у H1"
        );
        assert!(
            line.style.add_modifier.contains(Modifier::UNDERLINED),
            "UNDERLINED у H1"
        );
    }

    // ── inline_md_spans / inline_code_spans: смещения не в начале строки ──────
    #[test]
    fn inline_md_offsets_hold_for_mid_line_fragments() {
        // Код в СЕРЕДИНЕ строки: 146 (i+1→i-1) и 154 (i+=→i*=) разъедут спаны/хвост.
        let spans = inline_md_spans("aa `bb` cc");
        let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "aa bb cc");
        assert!(
            spans
                .iter()
                .any(|s| s.content == "bb" && s.style.fg == Some(Color::Indexed(180))),
            "код-фрагмент bb выделен точно"
        );

        // Одиночная «*» в середине не запускает разбор жирного (159: i+1→i*1).
        let spans = inline_md_spans("a * b **c** d");
        let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "a * b c d");
        assert!(
            spans
                .iter()
                .any(|s| s.content == "c" && s.style.add_modifier.contains(Modifier::BOLD)),
            "жирный c выделен, звёздочка литеральна"
        );
    }

    #[test]
    fn inline_code_at_line_start_has_no_empty_span() {
        // Бэктик в начале (open==0): 116 >→>= вставил бы пустой ведущий спан.
        let spans = inline_code_spans("`code` tail");
        assert_eq!(
            spans[0].content.as_ref(),
            "code",
            "нет пустого ведущего спана"
        );
        assert_eq!(spans[0].style.fg, Some(Color::Indexed(180)));
    }
}
