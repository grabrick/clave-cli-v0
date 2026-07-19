use super::*;

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
pub(crate) fn welcome_name_line(rest: &str) -> Line<'static> {
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
pub(crate) fn welcome_info_line(rest: &str) -> Line<'static> {
    match rest.split_once(WELCOME_SEP) {
        Some((logo, info)) => Line::from(vec![
            Span::styled(logo.to_string(), Style::default().fg(Color::Black)),
            Span::styled(format!("  {info}"), Style::default().fg(Color::Gray)),
        ]),
        None => Line::styled(rest.to_string(), Style::default().fg(Color::Black)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
