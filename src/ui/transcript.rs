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

// ── Welcome-блок (Claude-style: логотип слева + инфо справа, без рамок) ──────────
// Строки welcome помечены PUA-сентинелами (переживают санитайзинг, не встречаются в
// обычном контенте). `welcome_lines` (runtime.rs) их кодирует, функции ниже стилизуют.
pub(crate) const WELCOME_NAME: char = '\u{E010}'; // логотип | имя | версия
pub(crate) const WELCOME_INFO: char = '\u{E013}'; // логотип | текст (модель/cwd)
pub(crate) const WELCOME_HINT: char = '\u{E014}'; // строка-подсказка
pub(crate) const WELCOME_SEP: char = '\u{E011}'; // разделитель сегментов

/// Строка имени welcome: логотип чёрным, «clave» — белым жирным, версия — серым.
/// Цвет логотипа — ANSI чёрный (Terminal.app не поддерживает truecolor RGB →
/// рисовал бы белым), не зависит от темы.
fn welcome_name_line(rest: &str) -> Line<'static> {
    let mut parts = rest.split(WELCOME_SEP);
    let logo = parts.next().unwrap_or("").to_string();
    let name = parts.next().unwrap_or("").to_string();
    let version = parts.next().unwrap_or("").to_string();
    Line::from(vec![
        Span::styled(logo, Style::default().fg(Color::Black)),
        Span::styled(
            format!("  {name}"),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {version}"), Style::default().fg(Color::Gray)),
    ])
}

/// Строка welcome: логотип чёрным + (если есть) текст справа серым. Строки только
/// с логотипом (без разделителя) красятся целиком чёрным. Цвет логотипа не зависит
/// от темы.
fn welcome_info_line(rest: &str) -> Line<'static> {
    match rest.split_once(WELCOME_SEP) {
        Some((logo, info)) => Line::from(vec![
            Span::styled(logo.to_string(), Style::default().fg(Color::Black)),
            Span::styled(format!("  {info}"), Style::default().fg(Color::Gray)),
        ]),
        None => Line::styled(rest.to_string(), Style::default().fg(Color::Black)),
    }
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

/// Строка истории + гиперссылки на её спанах (индекс спана → доверенный URL).
pub(crate) struct RichLine {
    pub(crate) line: Line<'static>,
    pub(crate) links: Vec<SpanLink>,
}

pub(crate) struct SpanLink {
    pub(crate) span: usize,
    pub(crate) url: String,
}

/// Сегмент строки: текст + опциональная цель-файл (абс. путь, строка, колонка).
pub(crate) struct PathSeg {
    pub(crate) text: String,
    pub(crate) file: Option<(PathBuf, Option<u32>, Option<u32>)>,
}

fn is_path_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '/' | '_' | '-')
}

/// Считывает десятичное число с позиции `from`; вернёт (число, индекс-после).
fn take_number(chars: &[char], from: usize) -> Option<(u32, usize)> {
    let mut end = from;
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }
    if end == from {
        return None;
    }
    chars[from..end]
        .iter()
        .collect::<String>()
        .parse::<u32>()
        .ok()
        .map(|num| (num, end))
}

/// Резолвит токен в существующий файл: относительный — к `cwd`, абсолютный — как
/// есть. Требует наличие `/` (отсекает голые слова) и существование файла.
fn resolve_existing(path_str: &str, cwd: &Path) -> Option<PathBuf> {
    if !path_str.contains('/') {
        return None;
    }
    let candidate = if path_str.starts_with('/') {
        PathBuf::from(path_str)
    } else {
        cwd.join(path_str)
    };
    candidate.is_file().then_some(candidate)
}

/// Разбивает текст на сегменты, помечая токены-пути к существующим файлам.
/// Хвост `:line[:col]` распознаётся; хвостовая прозовая точка («…app.rs.») в
/// ссылку не входит. Сумма `text` сегментов равна исходному тексту.
pub(crate) fn detect_paths(text: &str, cwd: &Path) -> Vec<PathSeg> {
    let chars: Vec<char> = text.chars().collect();
    let mut segs: Vec<PathSeg> = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    while i < chars.len() {
        if !is_path_char(chars[i]) {
            plain.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && is_path_char(chars[i]) {
            i += 1;
        }
        let run: String = chars[start..i].iter().collect();
        // Хвостовую прозовую точку («…app.rs.») в путь не включаем. Двоеточие не
        // входит в path-charset, поэтому схемы (http://) распадаются на токены и
        // отсекаются проверкой существования файла ниже.
        let path_str = run.trim_end_matches('.');
        let Some(abs) = resolve_existing(path_str, cwd) else {
            plain.push_str(&run);
            continue;
        };

        let no_trailing_dot = path_str.len() == run.len();
        let (mut line, mut col, mut end) = (None, None, i);
        if no_trailing_dot && end < chars.len() && chars[end] == ':' {
            if let Some((parsed_line, after_line)) = take_number(&chars, end + 1) {
                line = Some(parsed_line);
                end = after_line;
                if end < chars.len() && chars[end] == ':' {
                    if let Some((parsed_col, after_col)) = take_number(&chars, end + 1) {
                        col = Some(parsed_col);
                        end = after_col;
                    }
                }
            }
        }

        if !plain.is_empty() {
            segs.push(PathSeg {
                text: std::mem::take(&mut plain),
                file: None,
            });
        }
        if no_trailing_dot {
            // Ссылка = путь [+ :line[:col]].
            segs.push(PathSeg {
                text: chars[start..end].iter().collect(),
                file: Some((abs, line, col)),
            });
            i = end;
        } else {
            // Ссылка только на путь; хвостовая пунктуация — обычный текст.
            segs.push(PathSeg {
                text: path_str.to_string(),
                file: Some((abs, None, None)),
            });
            plain.push_str(&run[path_str.len()..]);
        }
    }
    if !plain.is_empty() {
        segs.push(PathSeg {
            text: plain,
            file: None,
        });
    }
    segs
}

/// Пост-проход: режет спаны строки на под-спаны по найденным путям и навешивает
/// доверенный URL. При `Off` ссылок нет. ВАЖНО: печать (`render::queue_rich_line`)
/// применяет стили СПАНОВ, а не `line.style`, поэтому line-уровневый стиль
/// (`Line::styled` — ошибки, заголовки, строки welcome) складываем в каждый спан
/// (`base.patch(span.style)`), иначе он терялся бы и текст рисовался дефолтным белым.
pub(crate) fn attach_links(line: Line<'static>, cwd: &Path, target: PathTarget) -> RichLine {
    let base = line.style;
    if matches!(target, PathTarget::Off) {
        let spans = line
            .spans
            .into_iter()
            .map(|span| Span::styled(span.content, base.patch(span.style)))
            .collect::<Vec<_>>();
        return RichLine {
            line: Line::from(spans),
            links: Vec::new(),
        };
    }
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut links: Vec<SpanLink> = Vec::new();
    for span in line.spans {
        let style = base.patch(span.style);
        let segs = detect_paths(span.content.as_ref(), cwd);
        if segs.iter().all(|seg| seg.file.is_none()) {
            spans.push(Span::styled(span.content, style));
            continue;
        }
        for seg in segs {
            let index = spans.len();
            if let Some((abs, line_no, col)) = &seg.file {
                if let Some(url) = open_url(target, abs, *line_no, *col) {
                    links.push(SpanLink { span: index, url });
                }
            }
            spans.push(Span::styled(seg.text, style));
        }
    }
    RichLine {
        line: Line::from(spans),
        links,
    }
}

/// Как `history_line_render`, но с гиперссылками для печати в скроллбэк.
pub(crate) fn history_rich_render(
    line: &str,
    lang: Language,
    width: u16,
    theme: Theme,
    state: &mut TranscriptRenderState,
    target: PathTarget,
    cwd: &Path,
) -> Vec<RichLine> {
    transcript_entry_lines_with_state(line, lang, width, theme, state)
        .into_iter()
        .map(|rendered| attach_links(rendered, cwd, target))
        .collect()
}

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

    fn temp_repo(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("clave_paths_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).expect("temp src dir");
        fs::write(dir.join("src/app.rs"), "x").expect("temp file");
        dir
    }

    fn rejoin(segs: &[PathSeg]) -> String {
        segs.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn detect_links_existing_relative_path() {
        let cwd = temp_repo("rel");
        let segs = detect_paths("see src/app.rs now", &cwd);
        // Сегменты в сумме воспроизводят исходную строку.
        assert_eq!(rejoin(&segs), "see src/app.rs now");
        let link = segs.iter().find(|s| s.file.is_some()).expect("путь найден");
        assert_eq!(link.text, "src/app.rs");
        assert!(link.file.as_ref().unwrap().0.ends_with("src/app.rs"));
        let _ = fs::remove_dir_all(&cwd);
    }

    #[test]
    fn detect_parses_line_and_col() {
        let cwd = temp_repo("linecol");
        let segs = detect_paths("at src/app.rs:42:7 here", &cwd);
        assert_eq!(rejoin(&segs), "at src/app.rs:42:7 here");
        let link = segs.iter().find(|s| s.file.is_some()).unwrap();
        assert_eq!(link.text, "src/app.rs:42:7");
        let (_, line, col) = link.file.as_ref().unwrap();
        assert_eq!((*line, *col), (Some(42), Some(7)));
        let _ = fs::remove_dir_all(&cwd);
    }

    #[test]
    fn detect_excludes_trailing_sentence_dot() {
        let cwd = temp_repo("dot");
        let segs = detect_paths("open src/app.rs.", &cwd);
        assert_eq!(rejoin(&segs), "open src/app.rs.");
        let link = segs.iter().find(|s| s.file.is_some()).unwrap();
        assert_eq!(
            link.text, "src/app.rs",
            "хвостовая точка не входит в ссылку"
        );
        let _ = fs::remove_dir_all(&cwd);
    }

    #[test]
    fn detect_skips_nonexistent_urls_and_bare_words() {
        let cwd = temp_repo("skip");
        for text in [
            "no/such/file.rs",
            "https://example.com/x",
            "justaword",
            "Cargo",
        ] {
            let segs = detect_paths(text, &cwd);
            assert!(
                segs.iter().all(|s| s.file.is_none()),
                "{text:?} не должен линковаться"
            );
            assert_eq!(rejoin(&segs), text);
        }
        let _ = fs::remove_dir_all(&cwd);
    }

    #[test]
    fn attach_links_builds_url_and_off_disables() {
        let cwd = temp_repo("attach");
        let line = Line::from("⏺ edited src/app.rs ok");
        let rich = attach_links(line.clone(), &cwd, PathTarget::VsCode);
        assert_eq!(rich.links.len(), 1, "одна ссылка");
        assert!(rich.links[0].url.starts_with("vscode://file"));
        assert!(rich.links[0].url.contains("src/app.rs"));
        assert_eq!(
            rich.line.spans[rich.links[0].span].content.as_ref(),
            "src/app.rs"
        );
        // Off → линковка выключена.
        let off = attach_links(line, &cwd, PathTarget::Off);
        assert!(off.links.is_empty());
        let _ = fs::remove_dir_all(&cwd);
    }

    #[test]
    fn history_rich_render_links_paths_through_full_styling() {
        // Сквозной шов: стилизация ответа (⏺) + пост-проход линковки.
        let cwd = temp_repo("rich");
        let mut state = TranscriptRenderState::default();
        let rich = history_rich_render(
            "⏺ edited src/app.rs done",
            Language::Ru,
            80,
            Theme::Purple,
            &mut state,
            PathTarget::VsCode,
            &cwd,
        );
        let linked = rich
            .iter()
            .any(|row| row.links.iter().any(|l| l.url.contains("src/app.rs")));
        assert!(linked, "путь в ответе стал ссылкой через полный рендер");
        let _ = fs::remove_dir_all(&cwd);
    }

    #[test]
    fn welcome_name_line_logo_black_name_bold_version_gray() {
        // Сентинелы кодируют: логотип | имя | версия.
        let raw = format!("{WELCOME_NAME}LOGO{WELCOME_SEP}clave{WELCOME_SEP}v0.1.2");
        let line = style_transcript_line(&raw, Language::Ru, Theme::Purple);
        // Логотип — абсолютным чёрным (не зависит от темы).
        let logo = line.spans.first().expect("логотип-спан");
        assert_eq!(logo.content.as_ref(), "LOGO");
        assert_eq!(logo.style.fg, Some(Color::Black));
        // Имя — белым жирным.
        let name = line
            .spans
            .iter()
            .find(|s| s.content.contains("clave"))
            .expect("имя");
        assert_eq!(name.style.fg, Some(Color::White));
        assert!(name.style.add_modifier.contains(Modifier::BOLD));
        // Версия — серым; сентинелы не просочились в текст.
        let joined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "LOGO  clave  v0.1.2");
        assert!(!joined.contains(WELCOME_SEP) && !joined.contains(WELCOME_NAME));
    }

    #[test]
    fn welcome_info_line_logo_black_text_gray() {
        let raw = format!("{WELCOME_INFO}LOGO{WELCOME_SEP}~/proj");
        let line = style_transcript_line(&raw, Language::Ru, Theme::Purple);
        assert_eq!(line.spans[0].content.as_ref(), "LOGO");
        assert_eq!(line.spans[0].style.fg, Some(Color::Black));
        let joined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "LOGO  ~/proj");
    }

    #[test]
    fn welcome_renders_through_full_history_path() {
        // Сквозной путь (wrap + стилизация + attach_links): «clave» и логотип
        // печатаются, сентинелы вычищены.
        let line = format!("{WELCOME_NAME}▗▄▄▖{WELCOME_SEP}clave{WELCOME_SEP}v0.1.2");
        let mut state = TranscriptRenderState::default();
        let rich = history_rich_render(
            &line,
            Language::Ru,
            80,
            Theme::Purple,
            &mut state,
            PathTarget::Off,
            Path::new("/"),
        );
        let text: String = rich
            .iter()
            .flat_map(|row| row.line.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(text.contains("clave"), "welcome печатает 'clave': {text:?}");
        assert!(text.contains("▗▄▄▖"), "логотип на месте: {text:?}");
        assert!(
            !text.contains(WELCOME_NAME) && !text.contains(WELCOME_SEP),
            "сентинелы вычищены: {text:?}"
        );
    }

    #[test]
    fn attach_links_folds_line_style_into_spans() {
        // Регрессия: queue_rich_line печатает стили СПАНОВ, не line.style. attach_links
        // обязан сложить line.style в спан — иначе Line::styled (нижние строки лого,
        // подсказка, ошибки) терял цвет и рисовался дефолтным белым.
        let line = Line::styled("█▄▀ robot", Style::default().fg(Color::Black));
        let cwd = std::env::temp_dir();
        for target in [PathTarget::Off, PathTarget::VsCode] {
            let rich = attach_links(line.clone(), &cwd, target);
            let span = rich.line.spans.first().expect("спан есть");
            assert_eq!(
                span.style.fg,
                Some(Color::Black),
                "line.style сложен в спан (target={target:?})"
            );
        }
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

    // ── detect_paths: границы (потенциальные паники) и семантика ──────────────
    #[test]
    fn detect_paths_handles_tokens_at_end_of_text() {
        let cwd = temp_repo("borders");

        // Путь у самого конца строки: chars[end] за границей — 493:35 <→<= паникует.
        let segs = detect_paths("src/app.rs", &cwd);
        assert_eq!(rejoin(&segs), "src/app.rs");
        let link = segs.iter().find(|s| s.file.is_some()).unwrap();
        assert_eq!(link.text, "src/app.rs");
        assert_eq!(link.file.as_ref().unwrap().1, None, "без номера строки");

        // Число ПОСЛЕ пробела в конце: 493:28 &&→|| ошибочно зайдёт в разбор :line
        // и приклеит « 12» к ссылке с line=Some(12). Оригинал — путь без номера.
        let segs = detect_paths("src/app.rs 12", &cwd);
        assert_eq!(rejoin(&segs), "src/app.rs 12");
        let link = segs.iter().find(|s| s.file.is_some()).unwrap();
        assert_eq!(
            link.text, "src/app.rs",
            "число за пробелом не входит в ссылку"
        );
        let (_, line, col) = link.file.as_ref().unwrap();
        assert_eq!((*line, *col), (None, None));

        // :line в самом конце — 497:24 <→<= и 497:38 &&→|| лезут в chars[end].
        let segs = detect_paths("src/app.rs:12", &cwd);
        assert_eq!(rejoin(&segs), "src/app.rs:12");
        let link = segs.iter().find(|s| s.file.is_some()).unwrap();
        assert_eq!(link.text, "src/app.rs:12");
        let (_, line, col) = link.file.as_ref().unwrap();
        assert_eq!((*line, *col), (Some(12), None));

        // :line:col в самом конце.
        let segs = detect_paths("src/app.rs:12:5", &cwd);
        assert_eq!(rejoin(&segs), "src/app.rs:12:5");
        let link = segs.iter().find(|s| s.file.is_some()).unwrap();
        let (_, line, col) = link.file.as_ref().unwrap();
        assert_eq!((*line, *col), (Some(12), Some(5)));

        // Двоеточие без числа в конце: не съедается, файл распознан, паники нет.
        let segs = detect_paths("src/app.rs:", &cwd);
        assert_eq!(rejoin(&segs), "src/app.rs:");
        assert!(segs.iter().any(|s| s.file.is_some()), "файл распознан");

        let _ = fs::remove_dir_all(&cwd);
    }

    // ── take_number ──────────────────────────────────────────────────────────
    #[test]
    fn take_number_reads_digits_without_overrun() {
        // Число до конца среза: 435 <→<= полез бы в chars[len] и паникнул.
        assert_eq!(take_number(&['1', '2'], 0), Some((12, 2)));
        assert_eq!(take_number(&['a'], 0), None);
        assert_eq!(take_number(&['5', 'x'], 0), Some((5, 1)));
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
