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
