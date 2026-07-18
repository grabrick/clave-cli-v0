use super::*;

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
    use crate::ui::effort::testkit::*;

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
}
