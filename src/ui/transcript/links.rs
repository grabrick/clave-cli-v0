use super::*;

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
}
