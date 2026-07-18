use super::*;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::effort::testkit::*;

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
}
