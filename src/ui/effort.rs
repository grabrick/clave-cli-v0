use super::*;

pub(crate) fn draw_effort_screen(frame: &mut Frame<'_>, area: Rect, app: &App) {
    frame.render_widget(Clear, area);
    let tick = current_effort_tick();
    let scale_width = effort_scale_width(area.width);
    let scale_start = (area.width as usize).saturating_sub(scale_width) / 2;
    let mut lines = vec![Line::styled(
        "› /effort",
        Style::default()
            .fg(app.theme.accent())
            .add_modifier(Modifier::BOLD),
    )];
    lines.push(separator_line(area.width, app.theme));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        app.lang.choose("Усилие", "Effort"),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    if matches!(app.mode, Mode::ClaudeCodex | Mode::CodexClaude) {
        push_linked_effort_mode_line(&mut lines, area.width, app, tick);
    }
    push_effort_axis(&mut lines, area.width, scale_start, scale_width, app.lang);
    lines.push(Line::from(""));

    match app.mode {
        Mode::CodexOnly => push_effort_scale_block(
            &mut lines,
            EffortScaleBlock {
                width: area.width,
                scale_start,
                scale_width,
                title: "Codex",
                provider: "codex",
                allowed: CODEX_EFFORTS,
                selected_index: app.codex_effort_index,
                focused: true,
                tick,
                lang: app.lang,
                theme: app.theme,
            },
        ),
        Mode::ClaudeOnly => push_effort_scale_block(
            &mut lines,
            EffortScaleBlock {
                width: area.width,
                scale_start,
                scale_width,
                title: "Claude",
                provider: "claude",
                allowed: CLAUDE_EFFORTS,
                selected_index: app.claude_effort_index,
                focused: true,
                tick,
                lang: app.lang,
                theme: app.theme,
            },
        ),
        Mode::ClaudeCodex | Mode::CodexClaude => {
            if app.linked_effort_split {
                push_effort_scale_block(
                    &mut lines,
                    EffortScaleBlock {
                        width: area.width,
                        scale_start,
                        scale_width,
                        title: "Claude",
                        provider: "claude",
                        allowed: CLAUDE_EFFORTS,
                        selected_index: app.claude_effort_index,
                        focused: app.effort_focus == 1,
                        tick: tick + 2,
                        lang: app.lang,
                        theme: app.theme,
                    },
                );
                lines.push(Line::from(""));
                push_effort_scale_block(
                    &mut lines,
                    EffortScaleBlock {
                        width: area.width,
                        scale_start,
                        scale_width,
                        title: "Codex",
                        provider: "codex",
                        allowed: CODEX_EFFORTS,
                        selected_index: app.codex_effort_index,
                        focused: app.effort_focus == 2,
                        tick: tick + 4,
                        lang: app.lang,
                        theme: app.theme,
                    },
                );
            } else {
                push_effort_scale_block(
                    &mut lines,
                    EffortScaleBlock {
                        width: area.width,
                        scale_start,
                        scale_width,
                        title: app.lang.choose("Общий", "Shared"),
                        provider: "shared",
                        allowed: COMMON_EFFORTS,
                        selected_index: app.effort_index,
                        focused: app.effort_focus == 1,
                        tick: tick + 2,
                        lang: app.lang,
                        theme: app.theme,
                    },
                );
            }
        }
    }

    lines.push(Line::from(""));
    let hint = match app.mode {
        Mode::CodexOnly => app.lang.choose(
            "Codex: model_reasoning_effort, доступно low|medium|high|xhigh",
            "Codex: model_reasoning_effort, available low|medium|high|xhigh",
        ),
        Mode::ClaudeOnly => app.lang.choose(
            "Claude: --effort, доступно low|medium|high|max",
            "Claude: --effort, available low|medium|high|max",
        ),
        Mode::ClaudeCodex | Mode::CodexClaude if app.linked_effort_split => app.lang.choose(
            "Раздельно: Claude и Codex настраиваются независимо",
            "Per-model: Claude and Codex are adjusted independently",
        ),
        Mode::ClaudeCodex | Mode::CodexClaude => app.lang.choose(
            "Общий effort применяется к обеим моделям",
            "Shared effort is sent to both models",
        ),
    };
    lines.push(positioned_spans_line(
        area.width,
        vec![(
            scale_start,
            hint.chars().count(),
            vec![Span::styled(hint.to_string(), Style::default().fg(MUTED))],
        )],
    ));
    lines.push(Line::from(""));
    lines.push(Line::from(app.lang.choose(
        "↑/↓ выбрать · ←/→ настроить · Enter подтвердить · Esc отменить",
        "↑/↓ select · ←/→ adjust · Enter confirm · Esc cancel",
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

pub(crate) struct EffortScaleBlock<'a> {
    width: u16,
    scale_start: usize,
    scale_width: usize,
    title: &'a str,
    provider: &'a str,
    allowed: &'static [&'static str],
    selected_index: usize,
    focused: bool,
    tick: u64,
    lang: Language,
    theme: Theme,
}

pub(crate) fn push_effort_axis(
    lines: &mut Vec<Line<'static>>,
    width: u16,
    scale_start: usize,
    scale_width: usize,
    lang: Language,
) {
    let faster = lang.choose("Быстрее", "Faster");
    let smarter = lang.choose("Умнее", "Smarter");
    let smarter_pos = scale_start + scale_width.saturating_sub(smarter.chars().count());
    lines.push(positioned_spans_line(
        width,
        vec![
            (
                scale_start,
                faster.chars().count(),
                vec![Span::styled(
                    faster.to_string(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )],
            ),
            (
                smarter_pos,
                smarter.chars().count(),
                vec![Span::styled(
                    smarter.to_string(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )],
            ),
        ],
    ));
}

pub(crate) fn push_linked_effort_mode_line(
    lines: &mut Vec<Line<'static>>,
    width: u16,
    app: &App,
    tick: u64,
) {
    let focused = app.effort_focus == 0;
    let split_label = app.lang.choose("раздельно", "per-model");
    let common_label = app.lang.choose("общий", "shared");
    let title = app.lang.choose("Режим", "Mode");
    let prefix = if focused { "› " } else { "  " };
    let mut spans = vec![
        Span::styled(prefix, Style::default().fg(app.theme.accent())),
        Span::styled(
            format!("{title:<10} "),
            Style::default().fg(Color::White).add_modifier(if focused {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
        ),
    ];

    if app.linked_effort_split {
        spans.push(Span::styled(
            common_label.to_string(),
            Style::default().fg(MUTED),
        ));
        spans.push(Span::styled("  |  ", Style::default().fg(MUTED)));
        spans.extend(shimmer_text_spans(split_label, "xhigh", focused, tick));
    } else {
        spans.extend(shimmer_text_spans(common_label, "xhigh", focused, tick));
        spans.push(Span::styled("  |  ", Style::default().fg(MUTED)));
        spans.push(Span::styled(
            split_label.to_string(),
            Style::default().fg(MUTED),
        ));
    }
    lines.push(positioned_spans_line(
        width,
        vec![(0, visible_width_from_spans(&spans), spans)],
    ));
    lines.push(Line::from(""));
}

pub(crate) fn push_effort_scale_block(lines: &mut Vec<Line<'static>>, block: EffortScaleBlock<'_>) {
    let selected_effort = effort_label(block.selected_index);
    let prefix = if block.focused { "› " } else { "  " };
    lines.push(Line::from(vec![
        Span::styled(prefix, Style::default().fg(block.theme.accent())),
        Span::styled(
            format!("{:<8}", block.title),
            Style::default()
                .fg(Color::White)
                .add_modifier(if block.focused {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
        Span::styled(
            format!(
                " {}",
                effort_provider_hint(block.provider, block.lang, block.allowed)
            ),
            Style::default().fg(MUTED),
        ),
    ]));

    let tick_positions = effort_tick_positions(block.scale_width, block.allowed.len());
    lines.push(effort_scale_line(
        block.width,
        block.scale_start,
        block.scale_width,
        &tick_positions,
        block.theme,
    ));

    let mut label_items = Vec::new();
    for (index, effort) in block.allowed.iter().enumerate() {
        let selected = *effort == selected_effort;
        let label = effort_scale_label(effort, selected);
        let label_width = label.chars().count();
        let position = block.scale_start
            + tick_positions[index]
                .saturating_sub(label_width / 2)
                .min(block.scale_width.saturating_sub(label_width));
        label_items.push((
            position,
            label_width,
            effort_label_spans(
                &label,
                effort,
                selected && block.focused,
                block.tick + index as u64,
            ),
        ));
    }
    lines.push(positioned_spans_line(block.width, label_items));

    let description = effort_description(selected_effort, block.lang);
    lines.push(positioned_spans_line(
        block.width,
        vec![(
            block.scale_start,
            description.chars().count(),
            vec![Span::styled(
                description.to_string(),
                Style::default().fg(MUTED),
            )],
        )],
    ));
    lines.push(Line::from(""));
}

pub(crate) fn effort_provider_hint(provider: &str, lang: Language, allowed: &[&str]) -> String {
    match provider {
        "codex" => format!("model_reasoning_effort {}", allowed.join("|")),
        "claude" => format!("--effort {}", allowed.join("|")),
        _ => format!(
            "{} {}",
            lang.choose("доступно", "available"),
            allowed.join("|")
        ),
    }
}

pub(crate) fn visible_width_from_spans(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|span| span.content.chars().count()).sum()
}

pub(crate) fn effort_scale_width(width: u16) -> usize {
    let available = (width as usize).saturating_sub(8);
    available.clamp(32, 74)
}

pub(crate) fn effort_tick_positions(scale_width: usize, count: usize) -> Vec<usize> {
    if count <= 1 {
        return vec![0];
    }

    let last = scale_width.saturating_sub(1);
    (0..count).map(|index| index * last / (count - 1)).collect()
}

pub(crate) fn effort_scale_line(
    width: u16,
    scale_start: usize,
    scale_width: usize,
    tick_positions: &[usize],
    theme: Theme,
) -> Line<'static> {
    let width = width as usize;
    let mut cells = vec![' '; width];
    let end = (scale_start + scale_width).min(width);
    for cell in cells.iter_mut().take(end).skip(scale_start) {
        *cell = '─';
    }
    for tick in tick_positions {
        let position = scale_start + *tick;
        if position < width {
            cells[position] = '┬';
        }
    }

    Line::styled(
        cells.into_iter().collect::<String>(),
        Style::default().fg(theme.accent_dim()),
    )
}

pub(crate) fn positioned_spans_line(
    width: u16,
    mut items: Vec<(usize, usize, Vec<Span<'static>>)>,
) -> Line<'static> {
    let width = width as usize;
    items.sort_by_key(|(position, _, _)| *position);

    let mut spans = Vec::new();
    let mut cursor = 0usize;
    for (position, item_width, item_spans) in items {
        let position = position.min(width);
        if position < cursor {
            continue;
        }
        if position > cursor {
            spans.push(Span::raw(" ".repeat(position - cursor)));
        }
        spans.extend(item_spans);
        cursor = position.saturating_add(item_width);
    }

    Line::from(spans)
}

pub(crate) fn current_effort_tick() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| (duration.as_millis() / 120) as u64)
        .unwrap_or(0)
}

pub(crate) fn rotating_phase(seconds_per_phase: u64, phase_count: usize) -> usize {
    if phase_count == 0 {
        return 0;
    }

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| ((duration.as_secs() / seconds_per_phase) as usize) % phase_count)
        .unwrap_or(0)
}

pub(crate) fn effort_scale_label(effort: &str, selected: bool) -> String {
    let marker = if selected {
        match effort {
            "high" | "xhigh" | "max" => "✦",
            _ => "›",
        }
    } else {
        " "
    };
    format!("{marker} {effort:<6}")
}

pub(crate) fn effort_label_spans(
    label: &str,
    effort: &str,
    selected: bool,
    tick: u64,
) -> Vec<Span<'static>> {
    let animated = selected && matches!(effort, "high" | "xhigh" | "max");
    if !animated {
        return vec![Span::styled(
            label.to_string(),
            effort_style(effort, selected),
        )];
    }

    let mut spans = Vec::new();
    let chars = label.chars().collect::<Vec<_>>();
    for (index, ch) in chars.iter().enumerate() {
        let color = shimmer_color(effort, index, tick);
        spans.push(Span::styled(
            ch.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    }
    spans
}

pub(crate) fn shimmer_text_spans(
    text: &str,
    effort: &str,
    active: bool,
    tick: u64,
) -> Vec<Span<'static>> {
    if !active {
        return vec![Span::styled(text.to_string(), effort_style(effort, true))];
    }

    text.chars()
        .enumerate()
        .map(|(index, ch)| {
            Span::styled(
                ch.to_string(),
                Style::default()
                    .fg(shimmer_color(effort, index, tick))
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect()
}

pub(crate) fn shimmer_color(effort: &str, index: usize, tick: u64) -> Color {
    let palette: &[u8] = match effort {
        "high" => &[136, 178, 220, 214, 220, 178, 136],
        "xhigh" => &[97, 141, 183, 219, 183, 141, 97],
        "max" => &[160, 198, 203, 205, 203, 198, 160],
        _ => &[250],
    };
    let phase = (tick as usize) % palette.len();
    let color_index = (index + palette.len() - phase) % palette.len();
    Color::Indexed(palette[color_index])
}

pub(crate) fn effort_style(effort: &str, selected: bool) -> Style {
    let color = match effort {
        "low" => Color::Indexed(114),
        "medium" => Color::Indexed(117),
        "high" => Color::Indexed(220),
        "xhigh" => Color::Indexed(183),
        "max" => Color::Indexed(203),
        _ => MUTED,
    };

    let mut style = Style::default().fg(color);
    if selected {
        style = style.add_modifier(Modifier::BOLD);
    }
    style
}

pub(crate) fn effort_description(effort: &str, lang: Language) -> &'static str {
    match effort {
        "low" => lang.choose("быстро и экономно", "fast and frugal"),
        "medium" => lang.choose("баланс скорости и качества", "balanced speed and quality"),
        "high" => lang.choose("глубже думает над задачей", "deeper task reasoning"),
        "xhigh" => lang.choose("максимум Codex reasoning", "maximum Codex reasoning"),
        "max" => lang.choose("максимум Claude effort", "maximum Claude effort"),
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, buffer::Buffer};

    /// Палитра high — единственная, чьи цвета тест сверяет литералами. Держим её здесь,
    /// чтобы ожидание в тесте не приходило из той же функции, которую мы проверяем.
    const HIGH_PALETTE: [u8; 7] = [136, 178, 220, 214, 220, 178, 136];

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    fn span_texts(line: &Line<'_>) -> Vec<String> {
        line.spans
            .iter()
            .map(|span| span.content.to_string())
            .collect()
    }

    // ── Часть 1. Раскладка ───────────────────────────────────────────────────

    /// Шкала живёт в диапазоне 32..=74. Уйдёт нижняя граница — на узком терминале шкала
    /// схлопнется; уйдёт верхняя — на широком расползётся через весь экран.
    #[test]
    fn scale_width_is_terminal_minus_eight_clamped_to_32_74() {
        assert_eq!(effort_scale_width(10), 32, "нижняя граница обязана держать");
        assert_eq!(effort_scale_width(40), 32);
        assert_eq!(
            effort_scale_width(41),
            33,
            "внутри диапазона — ровно width-8"
        );
        assert_eq!(effort_scale_width(80), 72);
        assert_eq!(effort_scale_width(82), 74);
        assert_eq!(
            effort_scale_width(200),
            74,
            "верхняя граница обязана держать"
        );
    }

    /// Засечки раскладываются равномерно: первая в нуле, последняя ровно на конце шкалы.
    /// Числа — литералы: тест обязан знать ответ, а не пересчитывать формулу продукта.
    #[test]
    fn tick_positions_are_evenly_spread_from_zero_to_the_last_column() {
        assert_eq!(effort_tick_positions(32, 4), vec![0, 10, 20, 31]);
        assert_eq!(effort_tick_positions(72, 4), vec![0, 23, 47, 71]);
        assert_eq!(effort_tick_positions(72, 3), vec![0, 35, 71]);

        let ticks = effort_tick_positions(74, 5);
        assert_eq!(ticks.first().copied(), Some(0));
        assert_eq!(ticks.last().copied(), Some(73));
    }

    /// Одна засечка (и пустой список) не имеют «шага»: формула поделила бы на ноль.
    #[test]
    fn tick_positions_for_a_single_effort_collapse_to_zero() {
        assert_eq!(effort_tick_positions(32, 1), vec![0]);
        assert_eq!(effort_tick_positions(32, 0), vec![0]);
    }

    /// Строка шкалы: `┬` ровно на засечках, `─` между ними, за шкалой — воздух.
    #[test]
    fn scale_line_puts_ticks_exactly_on_the_tick_columns() {
        let line = effort_scale_line(80, 4, 72, &[0, 23, 47, 71], Theme::Purple);
        let text: Vec<char> = line_text(&line).chars().collect();

        assert_eq!(text.len(), 80, "строка обязана быть шириной с терминал");
        let ticks: Vec<usize> = text
            .iter()
            .enumerate()
            .filter(|(_, ch)| **ch == '┬')
            .map(|(index, _)| index)
            .collect();
        assert_eq!(ticks, vec![4, 27, 51, 75]);

        for (index, ch) in text.iter().enumerate() {
            let inside = (4..76).contains(&index);
            if inside {
                assert!(*ch == '─' || *ch == '┬', "внутри шкалы дыра на {index}");
            } else {
                assert_eq!(*ch, ' ', "за шкалой мусор на {index}");
            }
        }
    }

    /// Терминал уже шкалы: индексация ячеек обязана удержаться внутри строки.
    /// Без охраны `position < width` каждый кадр падал бы с index out of bounds.
    #[test]
    fn scale_line_on_a_narrow_terminal_stays_inside_the_row() {
        let line = effort_scale_line(10, 0, 32, &[0, 10, 20, 31], Theme::Purple);
        let text = line_text(&line);

        assert_eq!(text.chars().count(), 10);
        assert_eq!(text, "┬─────────");
    }

    /// Зазор между элементами — ровно расстояние до следующей позиции.
    #[test]
    fn positioned_line_pads_exactly_up_to_the_next_position() {
        let line = positioned_spans_line(
            30,
            vec![
                (0, 3, vec![Span::raw("abc")]),
                (10, 3, vec![Span::raw("xyz")]),
            ],
        );

        assert_eq!(line_text(&line), "abc       xyz");
        assert_eq!(span_texts(&line), vec!["abc", "       ", "xyz"]);
    }

    /// Элемент впритык к предыдущему сохраняется, и пустой распорки перед ним не появляется:
    /// лишний `Span::raw("")` текст не меняет, но ломает раскладку спанов.
    #[test]
    fn positioned_line_keeps_an_item_that_starts_exactly_at_the_cursor() {
        let line = positioned_spans_line(
            30,
            vec![
                (0, 3, vec![Span::raw("abc")]),
                (3, 3, vec![Span::raw("xyz")]),
            ],
        );

        assert_eq!(span_texts(&line), vec!["abc", "xyz"]);
    }

    /// Наезжающий элемент выбрасывается целиком — иначе он затёр бы соседа.
    #[test]
    fn positioned_line_drops_an_item_that_would_overlap() {
        let line = positioned_spans_line(
            30,
            vec![
                (0, 5, vec![Span::raw("abcde")]),
                (2, 3, vec![Span::raw("xyz")]),
            ],
        );

        assert_eq!(span_texts(&line), vec!["abcde"]);
    }

    /// Порядок в списке произволен — раскладка идёт по возрастанию позиции.
    #[test]
    fn positioned_line_sorts_items_by_position() {
        let line = positioned_spans_line(
            30,
            vec![
                (10, 3, vec![Span::raw("xyz")]),
                (0, 3, vec![Span::raw("abc")]),
            ],
        );

        assert_eq!(line_text(&line), "abc       xyz");
    }

    /// Позиция за краем прижимается к краю: отступ не может быть шире экрана.
    /// Сам текст не режется — обрезкой занимается Paragraph при отрисовке.
    #[test]
    fn positioned_line_clamps_a_position_beyond_the_screen() {
        let line = positioned_spans_line(
            10,
            vec![
                (0, 3, vec![Span::raw("abc")]),
                (100, 3, vec![Span::raw("xyz")]),
            ],
        );

        assert_eq!(span_texts(&line), vec!["abc", "       ", "xyz"]);
    }

    /// Ширина — в символах, а не в байтах: иначе кириллица и эмодзи разъедут раскладку.
    #[test]
    fn visible_width_counts_characters_not_bytes() {
        let spans = vec![Span::raw("привет"), Span::raw("🙂"), Span::raw("ok")];

        assert_eq!(visible_width_from_spans(&spans), 9);
        assert_eq!(visible_width_from_spans(&[]), 0);
    }

    // ── Часть 2. Цвета, метки, тексты ───────────────────────────────────────

    /// У каждого усилия свой цвет, и ни один не совпадает с «серым по умолчанию» —
    /// пропажа ветки match была бы не видна, если сверять только с чем-то одним.
    #[test]
    fn every_effort_has_its_own_colour() {
        let colors = [
            ("low", Color::Indexed(114)),
            ("medium", Color::Indexed(117)),
            ("high", Color::Indexed(220)),
            ("xhigh", Color::Indexed(183)),
            ("max", Color::Indexed(203)),
        ];

        for (effort, expected) in colors {
            let style = effort_style(effort, false);
            assert_eq!(style.fg, Some(expected), "цвет {effort}");
            assert_ne!(style.fg, Some(MUTED), "{effort} свалился в дефолт");
            assert!(!style.add_modifier.contains(Modifier::BOLD));
        }

        let unique: std::collections::HashSet<_> = colors.iter().map(|(_, c)| *c).collect();
        assert_eq!(unique.len(), 5, "цвета усилий обязаны различаться");

        assert_eq!(effort_style("wat", false).fg, Some(MUTED));
        assert!(effort_style("low", true)
            .add_modifier
            .contains(Modifier::BOLD));
    }

    /// Описание — своё у каждого усилия и на каждом языке; у неизвестного — пусто.
    #[test]
    fn every_effort_has_its_own_description_in_both_languages() {
        assert_eq!(effort_description("low", Language::Ru), "быстро и экономно");
        assert_eq!(
            effort_description("medium", Language::Ru),
            "баланс скорости и качества"
        );
        assert_eq!(
            effort_description("high", Language::Ru),
            "глубже думает над задачей"
        );
        assert_eq!(
            effort_description("xhigh", Language::Ru),
            "максимум Codex reasoning"
        );
        assert_eq!(
            effort_description("max", Language::Ru),
            "максимум Claude effort"
        );

        assert_eq!(effort_description("low", Language::En), "fast and frugal");
        assert_eq!(
            effort_description("medium", Language::En),
            "balanced speed and quality"
        );
        assert_eq!(
            effort_description("high", Language::En),
            "deeper task reasoning"
        );
        assert_eq!(
            effort_description("xhigh", Language::En),
            "maximum Codex reasoning"
        );
        assert_eq!(
            effort_description("max", Language::En),
            "maximum Claude effort"
        );

        assert_eq!(effort_description("wat", Language::Ru), "");

        let known: std::collections::HashSet<_> = EFFORTS
            .iter()
            .map(|effort| effort_description(effort, Language::Ru))
            .collect();
        assert_eq!(known.len(), 5, "описания обязаны различаться");
        assert!(
            !known.contains(""),
            "у известного усилия описание не пустое"
        );
    }

    /// Подпись под засечкой фиксированной ширины: поедет ширина — поедут все подписи.
    #[test]
    fn scale_label_has_a_marker_and_a_fixed_width() {
        assert_eq!(effort_scale_label("high", true), "✦ high  ");
        assert_eq!(effort_scale_label("xhigh", true), "✦ xhigh ");
        assert_eq!(effort_scale_label("max", true), "✦ max   ");
        assert_eq!(effort_scale_label("low", true), "› low   ");
        assert_eq!(effort_scale_label("medium", true), "› medium");
        assert_eq!(effort_scale_label("high", false), "  high  ");
        assert_eq!(effort_scale_label("low", false), "  low   ");

        for effort in EFFORTS {
            for selected in [true, false] {
                assert_eq!(
                    effort_scale_label(effort, selected).chars().count(),
                    8,
                    "подпись {effort} сменила ширину"
                );
            }
        }
    }

    /// Подсказка провайдера: у codex и claude — имя своего флага, у общего режима — перевод.
    #[test]
    fn provider_hint_names_the_flag_of_each_provider() {
        assert_eq!(
            effort_provider_hint("codex", Language::Ru, CODEX_EFFORTS),
            "model_reasoning_effort low|medium|high|xhigh"
        );
        assert_eq!(
            effort_provider_hint("claude", Language::Ru, CLAUDE_EFFORTS),
            "--effort low|medium|high|max"
        );
        assert_eq!(
            effort_provider_hint("shared", Language::Ru, COMMON_EFFORTS),
            "доступно low|medium|high"
        );
        assert_eq!(
            effort_provider_hint("shared", Language::En, COMMON_EFFORTS),
            "available low|medium|high"
        );
    }

    /// Точные цвета переливания: фаза едет от тика, волна идёт по символам.
    #[test]
    fn shimmer_colour_walks_the_palette_with_the_tick() {
        assert_eq!(shimmer_color("high", 0, 0), Color::Indexed(136));
        assert_eq!(shimmer_color("high", 1, 0), Color::Indexed(178));
        assert_eq!(shimmer_color("high", 2, 0), Color::Indexed(220));
        assert_eq!(shimmer_color("high", 3, 0), Color::Indexed(214));

        // Фаза сдвигает волну: тот же символ — другой цвет на следующем тике.
        assert_eq!(shimmer_color("high", 2, 1), Color::Indexed(178));
        assert_eq!(shimmer_color("high", 2, 2), Color::Indexed(136));
        assert_eq!(shimmer_color("high", 2, 5), Color::Indexed(220));
        assert_ne!(shimmer_color("high", 2, 0), shimmer_color("high", 2, 1));

        // Волна цикличная: период — длина палитры.
        for index in 0..8 {
            assert_eq!(
                shimmer_color("high", index, 3),
                shimmer_color("high", index, 3 + 7),
                "цикл сорвался на символе {index}"
            );
        }
    }

    /// У каждого усилия своя палитра, у неизвестного — ровный серый на любом тике.
    #[test]
    fn each_animated_effort_has_its_own_palette() {
        let high = shimmer_color("high", 1, 0);
        let xhigh = shimmer_color("xhigh", 1, 0);
        let max = shimmer_color("max", 1, 0);

        assert_eq!(high, Color::Indexed(178));
        assert_eq!(xhigh, Color::Indexed(141));
        assert_eq!(max, Color::Indexed(198));
        assert_ne!(high, xhigh);
        assert_ne!(high, max);
        assert_ne!(xhigh, max);

        for tick in [0u64, 1, 6, 7, 1_000_000] {
            for index in 0..8 {
                assert_eq!(
                    shimmer_color("low", index, tick),
                    Color::Indexed(250),
                    "неанимированное усилие обязано быть ровным"
                );
            }
        }
    }

    /// Тик приходит от стенных часов и растёт бесконечно. Фаза обязана остаться внутри
    /// палитры: без взятия по модулю следующая же строка уходит в underflow и валит кадр.
    #[test]
    fn shimmer_colour_survives_huge_ticks() {
        for tick in [u64::MAX, u64::MAX - 1, 1 << 62, 999_999_999_999] {
            for index in 0..8 {
                let Color::Indexed(value) = shimmer_color("high", index, tick) else {
                    panic!("палитра обязана давать индексированный цвет");
                };
                assert!(
                    HIGH_PALETTE.contains(&value),
                    "цвет {value} вне палитры на тике {tick}"
                );
            }
        }
    }

    /// Анимируется только выбранное усилие из «умных». Всё прочее — один ровный спан:
    /// иначе на экране переливалось бы всё подряд, а текст при этом не изменился бы.
    #[test]
    fn label_spans_animate_only_the_selected_smart_effort() {
        let plain = effort_label_spans("  low   ", "low", true, 0);
        assert_eq!(plain.len(), 1, "выбранный low не анимируется");
        assert_eq!(plain[0].content.as_ref(), "  low   ");
        assert_eq!(plain[0].style, effort_style("low", true));

        let unselected = effort_label_spans("  high  ", "high", false, 0);
        assert_eq!(unselected.len(), 1, "невыбранный high не анимируется");
        assert_eq!(unselected[0].style, effort_style("high", false));

        let animated = effort_label_spans("✦ high  ", "high", true, 0);
        assert_eq!(animated.len(), 8, "анимация — посимвольная");
        assert_eq!(animated[0].content.as_ref(), "✦");
        assert_eq!(animated[0].style.fg, Some(Color::Indexed(136)));
        assert_eq!(animated[1].style.fg, Some(Color::Indexed(178)));
        assert!(animated
            .iter()
            .all(|span| span.style.add_modifier.contains(Modifier::BOLD)));
    }

    /// Ровно так же — у текста режима: неактивный отдаётся одним спаном.
    #[test]
    fn shimmer_text_spans_split_per_character_only_when_active() {
        let idle = shimmer_text_spans("общий", "xhigh", false, 0);
        assert_eq!(idle.len(), 1);
        assert_eq!(idle[0].content.as_ref(), "общий");
        assert_eq!(idle[0].style, effort_style("xhigh", true));

        let active = shimmer_text_spans("общий", "xhigh", true, 0);
        assert_eq!(active.len(), 5, "по символу на спан");
        assert_eq!(
            active
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>(),
            "общий"
        );
        assert_eq!(active[0].style.fg, Some(Color::Indexed(97)));
        assert_eq!(active[1].style.fg, Some(Color::Indexed(141)));
        assert!(active
            .iter()
            .all(|span| span.style.add_modifier.contains(Modifier::BOLD)));
    }

    // ── Часть 3. Часы ───────────────────────────────────────────────────────

    fn epoch_millis() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("часы не до эпохи")
            .as_millis()
    }

    /// Тик анимации — стенные часы, поделённые на шаг кадра. Не «какое-то число»:
    /// сверяем с эталоном, посчитанным здесь же, с допуском на пару кадров.
    #[test]
    fn effort_tick_follows_the_wall_clock() {
        let before = epoch_millis() / 120;
        let got = current_effort_tick() as u128;
        let after = epoch_millis() / 120;

        assert!(
            got >= before && got <= after,
            "тик разошёлся с часами: {got} вне [{before}, {after}]"
        );
        assert!(
            got > 1,
            "тик обязан быть настоящим временем, а не константой"
        );
    }

    /// Фаза вращения — тоже часы. Счётчик берём большим, чтобы эталон заведомо не
    /// совпал ни с нулём, ни с единицей: иначе мутант-константа прошёл бы незамеченным.
    #[test]
    fn rotating_phase_follows_the_wall_clock() {
        let phase_count = 1_000_000_000usize;
        let expected = |secs: u128| ((secs / 2) as usize) % phase_count;

        let before = expected(epoch_millis() / 1000);
        let got = rotating_phase(2, phase_count);
        let after = expected(epoch_millis() / 1000);

        assert!(
            got == before || got == after,
            "фаза разошлась с часами: {got} не {before} и не {after}"
        );
        assert!(got > 1, "фаза обязана быть настоящим временем");
    }

    /// Пустой список фаз — не деление на ноль, а честный ноль. И фаза всегда внутри списка.
    #[test]
    fn rotating_phase_is_zero_without_phases_and_always_inside_the_range() {
        assert_eq!(rotating_phase(2, 0), 0);

        for count in 1..=8usize {
            let phase = rotating_phase(1, count);
            assert!(phase < count, "фаза {phase} вылезла за {count}");
        }
    }

    // ── Часть 4. Блоки и экран ──────────────────────────────────────────────

    fn effort_app() -> App {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = std::env::temp_dir().join(format!(
            "clave-effort-{}-{}",
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
        app.lang = Language::Ru;
        app.theme = Theme::Purple;
        app
    }

    fn screen(app: &App, width: u16, height: u16) -> Buffer {
        let mut terminal =
            Terminal::new(TestBackend::new(width, height)).expect("оффскрин-терминал");
        terminal
            .draw(|frame| draw_effort_screen(frame, frame.area(), app))
            .expect("отрисовка экрана");
        terminal.backend().buffer().clone()
    }

    fn buffer_rows(buffer: &Buffer) -> Vec<String> {
        let area = buffer.area;
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .filter_map(|x| buffer.cell((x, y)))
                    .map(|cell| cell.symbol())
                    .collect::<String>()
            })
            .collect()
    }

    fn row_with(rows: &[String], needle: &str) -> String {
        rows.iter()
            .find(|row| row.contains(needle))
            .unwrap_or_else(|| panic!("на экране нет строки с «{needle}»:\n{}", rows.join("\n")))
            .clone()
    }

    /// Ось шкалы: «Быстрее» слева, «Умнее» прижато к правому концу шкалы.
    /// Сползёт правая подпись — шкала перестанет читаться как направление.
    #[test]
    fn axis_pins_smarter_to_the_right_end_of_the_scale() {
        let mut lines = Vec::new();
        push_effort_axis(&mut lines, 80, 4, 72, Language::Ru);

        assert_eq!(lines.len(), 1);
        let text = line_text(&lines[0]);
        assert!(
            text.starts_with("    Быстрее"),
            "ось начинается с отступа шкалы"
        );
        assert_eq!(
            text.chars().count(),
            4 + 7 + 60 + 5,
            "ось: отступ, «Быстрее», зазор, «Умнее»"
        );
        assert!(text.ends_with("Умнее"));

        let mut english = Vec::new();
        push_effort_axis(&mut english, 80, 4, 72, Language::En);
        let text = line_text(&english[0]);
        assert_eq!(
            text,
            format!("{}Faster{}Smarter", " ".repeat(4), " ".repeat(59))
        );
    }

    /// Строка режима: подсвечен ТОТ вариант, который включён, а второй — серый.
    #[test]
    fn mode_line_highlights_the_active_variant() {
        let mut app = effort_app();
        app.effort_focus = 0;
        app.linked_effort_split = true;

        let mut lines = Vec::new();
        push_linked_effort_mode_line(&mut lines, 80, &app, 0);
        assert_eq!(lines.len(), 2, "строка режима и воздух под ней");
        let line = &lines[0];
        assert_eq!(line_text(line), "› Режим      общий  |  раздельно");
        let shared = line
            .spans
            .iter()
            .find(|span| span.content.as_ref() == "общий")
            .expect("невыбранный «общий» идёт одним спаном");
        assert_eq!(shared.style.fg, Some(MUTED));
        assert_eq!(
            line.spans
                .iter()
                .filter(|span| span.style.add_modifier.contains(Modifier::BOLD))
                .count(),
            10,
            "заголовок и девять символов «раздельно» переливаются"
        );

        app.linked_effort_split = false;
        let mut lines = Vec::new();
        push_linked_effort_mode_line(&mut lines, 80, &app, 0);
        let line = &lines[0];
        assert_eq!(line_text(line), "› Режим      общий  |  раздельно");
        let split = line
            .spans
            .iter()
            .find(|span| span.content.as_ref() == "раздельно")
            .expect("невыбранное «раздельно» идёт одним спаном");
        assert_eq!(split.style.fg, Some(MUTED));
    }

    /// Маркер `›` стоит там, где фокус. Ушёл фокус со строки режима — ушёл и маркер.
    #[test]
    fn mode_line_marker_follows_the_focus() {
        let mut app = effort_app();
        app.linked_effort_split = true;

        app.effort_focus = 0;
        let mut focused = Vec::new();
        push_linked_effort_mode_line(&mut focused, 80, &app, 0);
        assert!(line_text(&focused[0]).starts_with("› "));

        app.effort_focus = 1;
        let mut blurred = Vec::new();
        push_linked_effort_mode_line(&mut blurred, 80, &app, 0);
        let text = line_text(&blurred[0]);
        assert!(text.starts_with("  "), "без фокуса маркера нет: {text:?}");
        assert_eq!(
            blurred[0]
                .spans
                .iter()
                .filter(|span| span.content.as_ref() == "раздельно")
                .count(),
            1,
            "без фокуса переливание выключено"
        );
    }

    fn claude_block(tick: u64, focused: bool) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        push_effort_scale_block(
            &mut lines,
            EffortScaleBlock {
                width: 80,
                scale_start: 4,
                scale_width: 72,
                title: "Claude",
                provider: "claude",
                allowed: CLAUDE_EFFORTS,
                selected_index: effort_index_for("high"),
                focused,
                tick,
                lang: Language::Ru,
                theme: Theme::Purple,
            },
        );
        lines
    }

    /// Блок шкалы целиком: заголовок, шкала, подписи под засечками, описание, воздух.
    /// Подписи стоят на своих колонках и не наезжают друг на друга.
    #[test]
    fn scale_block_lays_out_header_scale_labels_and_description() {
        let lines = claude_block(0, true);

        assert_eq!(
            lines.len(),
            5,
            "заголовок, шкала, подписи, описание, воздух"
        );
        assert_eq!(
            line_text(&lines[0]),
            "› Claude   --effort low|medium|high|max"
        );

        let scale: Vec<usize> = line_text(&lines[1])
            .chars()
            .enumerate()
            .filter(|(_, ch)| *ch == '┬')
            .map(|(index, _)| index)
            .collect();
        assert_eq!(scale, vec![4, 27, 51, 75]);

        let labels = line_text(&lines[2]);
        assert_eq!(
            labels,
            format!(
                "{}  low   {}  medium{}✦ high  {}  max   ",
                " ".repeat(4),
                " ".repeat(11),
                " ".repeat(16),
                " ".repeat(13)
            )
        );
        assert_eq!(
            line_text(&lines[3]),
            format!("{}глубже думает над задачей", " ".repeat(4))
        );
        assert_eq!(line_text(&lines[4]), "");
    }

    /// Переливается ровно одна подпись — выбранная и только в блоке под фокусом.
    /// Текст при этом не меняется, поэтому проверяем спаны и их цвета.
    #[test]
    fn scale_block_animates_only_the_selected_label() {
        let lines = claude_block(0, true);
        let labels = &lines[2];

        // 4 распорки + low + medium + 8 символов high + max.
        assert_eq!(labels.spans.len(), 15);

        let animated: Vec<Color> = labels
            .spans
            .iter()
            .skip(5)
            .take(8)
            .map(|span| span.style.fg.expect("у переливания есть цвет"))
            .collect();
        // Подпись high — третья в списке, значит её тик = block.tick + 2.
        let phase = 2usize;
        let expected: Vec<Color> = (0..8)
            .map(|index| Color::Indexed(HIGH_PALETTE[(index + 7 - phase) % 7]))
            .collect();
        assert_eq!(animated, expected, "волна обязана ехать от тика блока");

        let unfocused = claude_block(0, false);
        assert_eq!(
            unfocused[2].spans.len(),
            8,
            "без фокуса подписи не переливаются"
        );
        assert!(line_text(&unfocused[0]).starts_with("  "), "и маркера нет");
    }

    /// Выбор влияет и на маркер, и на описание: они обязаны говорить об одном усилии.
    #[test]
    fn scale_block_marks_and_describes_the_selected_effort() {
        let mut lines = Vec::new();
        push_effort_scale_block(
            &mut lines,
            EffortScaleBlock {
                width: 80,
                scale_start: 4,
                scale_width: 72,
                title: "Codex",
                provider: "codex",
                allowed: CODEX_EFFORTS,
                selected_index: effort_index_for("low"),
                focused: true,
                tick: 0,
                lang: Language::En,
                theme: Theme::Purple,
            },
        );

        let labels = line_text(&lines[2]);
        assert!(labels.contains("› low"), "выбранный low получает маркер");
        assert!(!labels.contains("✦"), "у low маркера «умного» усилия нет");
        assert!(labels.contains("  medium"), "невыбранные — без маркера");
        assert_eq!(
            line_text(&lines[3]),
            format!("{}fast and frugal", " ".repeat(4))
        );
    }

    /// Экран одиночного Claude: свой заголовок, свой флаг и ни следа Codex.
    #[test]
    fn screen_for_claude_only_shows_claude_and_its_flag() {
        let mut app = effort_app();
        app.mode = Mode::ClaudeOnly;

        let rows = buffer_rows(&screen(&app, 80, 30));
        let text = rows.join("\n");

        assert!(text.contains("/effort"), "экран пуст");
        assert!(text.contains("Claude"));
        assert!(text.contains("--effort"));
        assert!(!text.contains("Codex"), "в одиночном Claude нет Codex");
        assert!(
            !text.contains("Режим"),
            "строки режима в одиночном режиме нет"
        );
    }

    /// Экран одиночного Codex — зеркально: свой флаг и никакой строки режима.
    #[test]
    fn screen_for_codex_only_shows_codex_and_its_flag() {
        let mut app = effort_app();
        app.mode = Mode::CodexOnly;

        let rows = buffer_rows(&screen(&app, 80, 30));
        let text = rows.join("\n");

        assert!(text.contains("Codex"));
        assert!(text.contains("model_reasoning_effort"));
        assert!(!text.contains("Режим"));
        assert!(text.contains("Codex: model_reasoning_effort, доступно"));
    }

    /// Раздельный режим: два блока и подсказка именно про раздельность.
    #[test]
    fn screen_in_split_mode_shows_two_blocks_and_the_split_hint() {
        let mut app = effort_app();
        app.mode = Mode::ClaudeCodex;
        app.linked_effort_split = true;

        let rows = buffer_rows(&screen(&app, 80, 40));
        let text = rows.join("\n");

        assert!(text.contains("Claude"), "нет блока Claude");
        assert!(text.contains("Codex"), "нет блока Codex");
        assert!(text.contains("Режим"));
        assert!(
            text.contains("Раздельно: Claude и Codex настраиваются независимо"),
            "подсказка не про раздельный режим:\n{text}"
        );
        assert!(!text.contains("Общий effort применяется"));
    }

    /// Общий режим: один блок и подсказка про общий effort.
    #[test]
    fn screen_in_shared_mode_shows_one_block_and_the_shared_hint() {
        let mut app = effort_app();
        app.mode = Mode::CodexClaude;
        app.linked_effort_split = false;

        let rows = buffer_rows(&screen(&app, 80, 40));
        let text = rows.join("\n");

        assert!(text.contains("Общий"), "нет общего блока");
        assert!(
            text.contains("Общий effort применяется к обеим моделям"),
            "подсказка не про общий режим:\n{text}"
        );
        assert!(!text.contains("Раздельно: Claude"));
        assert!(
            !text.contains("--effort low"),
            "в общем режиме блок один, флагов провайдеров нет"
        );
    }

    /// Шкала отцентрована по экрану: засечки стоят ровно на своих колонках.
    #[test]
    fn screen_centres_the_scale() {
        let mut app = effort_app();
        app.mode = Mode::ClaudeOnly;

        let rows = buffer_rows(&screen(&app, 80, 30));
        let scale = row_with(&rows, "┬");
        let ticks: Vec<usize> = scale
            .chars()
            .enumerate()
            .filter(|(_, ch)| *ch == '┬')
            .map(|(index, _)| index)
            .collect();

        assert_eq!(ticks, vec![4, 27, 51, 75]);
    }

    /// Узкий терминал: шкала шире экрана, и отрисовка обязана это пережить.
    #[test]
    fn screen_survives_a_terminal_narrower_than_the_scale() {
        let mut app = effort_app();
        app.mode = Mode::ClaudeCodex;
        app.linked_effort_split = true;

        let rows = buffer_rows(&screen(&app, 10, 30));

        assert_eq!(rows.len(), 30);
        assert!(rows.iter().any(|row| row.contains("effort")));
    }

    /// Фокус в раздельном режиме переезжает между блоками — маркер обязан ехать с ним.
    #[test]
    fn screen_moves_the_marker_between_blocks_with_the_focus() {
        let mut app = effort_app();
        app.mode = Mode::ClaudeCodex;
        app.linked_effort_split = true;

        app.effort_focus = 1;
        let rows = buffer_rows(&screen(&app, 80, 40));
        assert!(row_with(&rows, "Claude  ").starts_with("› "));
        assert!(row_with(&rows, "Codex   ").starts_with("  "));

        app.effort_focus = 2;
        let rows = buffer_rows(&screen(&app, 80, 40));
        assert!(row_with(&rows, "Claude  ").starts_with("  "));
        assert!(row_with(&rows, "Codex   ").starts_with("› "));
    }

    /// В общем режиме блок один, но фокус на нём такой же настоящий: на строке режима
    /// маркера нет, а на блоке — есть.
    #[test]
    fn screen_marks_the_shared_block_when_it_holds_the_focus() {
        let mut app = effort_app();
        app.mode = Mode::ClaudeCodex;
        app.linked_effort_split = false;

        app.effort_focus = 1;
        let rows = buffer_rows(&screen(&app, 80, 40));
        assert!(row_with(&rows, "Общий").starts_with("› "));
        assert!(row_with(&rows, "Режим").starts_with("  "));

        app.effort_focus = 0;
        let rows = buffer_rows(&screen(&app, 80, 40));
        assert!(row_with(&rows, "Общий").starts_with("  "));
        assert!(row_with(&rows, "Режим").starts_with("› "));
    }

    /// Цвета переливания в блоке под фокусом: снимаем тик вокруг кадра и требуем ровно ту
    /// фазу, которую даёт смещение блока. Ошибка в смещении сдвигает всю волну — и это
    /// видно только по полному вектору цветов, потому что палитра симметрична.
    fn assert_block_shimmer(mode_split: bool, focus: usize, block_offset: u64) {
        let mut app = effort_app();
        app.mode = Mode::ClaudeCodex;
        app.linked_effort_split = mode_split;
        app.effort_focus = focus;
        // Оставляем ровно одну переливающуюся подпись: у соседнего блока выбран low.
        app.claude_effort_index = effort_index_for(if focus == 2 { "low" } else { "high" });
        app.codex_effort_index = effort_index_for(if focus == 2 { "high" } else { "low" });
        app.effort_index = effort_index_for("high");

        // Смещение блока обязано отличаться и от сдвига (tick - k), и от произведения
        // (tick * k) по модулю длины палитры. Второе верно не при любом тике — ждём удобный.
        for _ in 0..80 {
            let tick = current_effort_tick();
            let base = (tick + block_offset + 2) % 7;
            let doubled = (tick * block_offset + 2) % 7;
            if base == doubled {
                thread::sleep(Duration::from_millis(40));
                continue;
            }

            let buffer = screen(&app, 80, 40);
            if current_effort_tick() != tick {
                continue; // тик перевалил прямо во время кадра — кадр не сравнить
            }

            let rows = buffer_rows(&buffer);
            let (y, row) = rows
                .iter()
                .enumerate()
                .find(|(_, row)| row.contains('✦'))
                .expect("переливающаяся подпись обязана быть на экране");
            let x = row.chars().position(|ch| ch == '✦').expect("маркер найден");

            let colors: Vec<Color> = (0..8)
                .map(|offset| {
                    buffer
                        .cell(((x + offset) as u16, y as u16))
                        .expect("ячейка подписи")
                        .fg
                })
                .collect();
            let expected: Vec<Color> = (0..8)
                .map(|index| Color::Indexed(HIGH_PALETTE[(index + 7 - base as usize) % 7]))
                .collect();

            assert_eq!(
                colors, expected,
                "фаза блока разошлась (тик {tick}, смещение {block_offset})"
            );
            return;
        }
        panic!("не дождались тика, на котором фазу можно проверить");
    }

    /// Блок Claude в раздельном режиме анимируется от тика со смещением +2.
    #[test]
    fn screen_shifts_the_claude_block_phase_by_two() {
        assert_block_shimmer(true, 1, 2);
    }

    /// Блок Codex — со смещением +4: два блока не должны переливаться в такт.
    #[test]
    fn screen_shifts_the_codex_block_phase_by_four() {
        assert_block_shimmer(true, 2, 4);
    }

    /// Общий блок — со смещением +2.
    #[test]
    fn screen_shifts_the_shared_block_phase_by_two() {
        assert_block_shimmer(false, 1, 2);
    }
}
