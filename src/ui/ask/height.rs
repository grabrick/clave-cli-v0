use super::*;

/// Высота панели селектора: рамка + (степпер) + контент + подсказка.
pub(crate) fn ask_panel_height(state: &AskState, width: u16, cap: u16) -> u16 {
    let stepper = u16::from(state.multi_question());
    let iw = (width as usize).saturating_sub(2).max(8); // минус рамка
    let body = if state.on_confirm() {
        (state.confirm_rows() as u16).min(12)
    } else if let Some(question) = state.question() {
        // Высота с учётом переноса: строка(и) вопроса + варианты + «Свой ответ» (если он есть).
        let mut rows = wrapped_rows(display_width(&question.question), iw);
        for opt in &question.options {
            // ~6 символов на маркер/номер/чекбокс перед текстом варианта.
            rows += wrapped_rows(display_width(&opt.label) + 6, iw);
        }
        rows += usize::from(question.allow_custom);
        rows as u16
    } else {
        1
    };
    (2 + stepper + body + 1).min(cap).max(4)
}

/// Сколько визуальных строк займёт текст длиной `chars` при ширине `width`.
fn wrapped_rows(chars: usize, width: usize) -> usize {
    if width == 0 {
        return 1;
    }
    chars.max(1).div_ceil(width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::ask::testkit::*;

    /// Высота переноса — точным числом. Нулевая ширина не делит на ноль (26:14),
    /// пустой текст — всё равно строка (нижняя граница max(1)).
    #[test]
    fn wrapped_rows_counts_visual_lines_and_survives_zero_width() {
        assert_eq!(wrapped_rows(0, 10), 1);
        assert_eq!(wrapped_rows(1, 10), 1);
        assert_eq!(wrapped_rows(10, 4), 3);
        assert_eq!(wrapped_rows(8, 4), 2);
        assert_eq!(wrapped_rows(5, 0), 1); // 26:14 == -> != упал бы в div_ceil(0)
        assert_eq!(wrapped_rows(20, 10), 2); // ≠1 → мутанты 26:5 -> 0/1 падают
    }

    /// Высота панели — литералами. Каждое слагаемое формулы (степпер, тело, рамка,
    /// подсказка) закрыто своим сравнением; сдвиг любого поменяет число.
    #[test]
    fn panel_height_is_frame_plus_stepper_plus_body() {
        // Один вопрос, 3 варианта, без «своего ответа»: тело = вопрос(1) + 3·вариант(1) = 4.
        let base = ask_state(vec![question("Q?", false, &["A", "B", "C"], false)], 0);
        assert_eq!(ask_panel_height(&base, 40, 100), 7);

        // +1 на строку «Свой ответ» (16:14 += ).
        let custom = ask_state(vec![question("Q?", false, &["A", "B", "C"], true)], 0);
        assert_eq!(ask_panel_height(&custom, 40, 100), 8);

        // +1 на степпер при нескольких вопросах (21:8 «2 + stepper»).
        let multi = ask_state(
            vec![
                question("Q?", false, &["A", "B", "C"], false),
                question("Q1", false, &["A", "B", "C"], false),
            ],
            0,
        );
        assert_eq!(ask_panel_height(&multi, 40, 100), 8);

        // Узкая панель: длинная метка переносится (14:18 += и 14:60 «label + 6»).
        let narrow = ask_state(vec![question("Q", false, &["MMMMMMMMMM"], false)], 0);
        assert_eq!(ask_panel_height(&narrow, 10, 100), 6);

        // Шаг подтверждения: тело = confirm_rows().min(12) = 3.
        let confirm = ask_state(
            vec![
                question("Q?", false, &["A", "B"], true),
                question("Q1", false, &["C", "D"], true),
            ],
            2,
        );
        assert_eq!(ask_panel_height(&confirm, 40, 100), 7);

        // Потолок cap и пол 4 (21 .min(cap).max(4)).
        assert_eq!(ask_panel_height(&custom, 40, 5), 5);
        assert_eq!(ask_panel_height(&custom, 40, 2), 4);
    }

    // ── Часть 2. Мелкие функции ──────────────────────────────────────────────
}
