use super::*;

/// Максимум колонок под имя ветки: длинная `feature/...` иначе вытеснила бы индикатор
/// с экрана целиком уже на 100 колонках.
const GIT_REF_MAX_COLS: usize = 20;
/// Разделитель между git-индикатором и вращающимся правым слотом.
pub(crate) const GIT_GAP: usize = 2;

/// Готовые ширины футера. Все значения — в КОЛОНКАХ терминала (не в символах): `→`, `·`
/// и кириллица в правом слоте иначе разъезжаются с тем, что реально рисует терминал.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FooterLayout {
    pub(crate) hints: String,
    /// Пусто — индикатор не помещается (или репозитория нет) и не рисуется.
    pub(crate) git: String,
    pub(crate) gap: usize,
    pub(crate) right: String,
    pub(crate) right_padding: usize,
    /// Ширина слота вращающегося сегмента: он не двигается от появления индикатора.
    pub(crate) right_slot_width: usize,
}

pub(crate) fn footer_layout(
    width: usize,
    mode_label: &str,
    switch: &str,
    hints: &str,
    git: Option<&str>,
    right: &str,
    right_slot_width: usize,
) -> FooterLayout {
    // Держим запас у правой стены и НЕ дорисовываем до последней колонки. Рендер печатает
    // строку по НАШЕЙ ширине (unicode-width считает `→`/`·` за 1 клетку), но терминал
    // рисует такие «неоднозначные по ширине» символы в 2 клетки, плюс крайняя ячейка
    // страдает от last-column-quirk — и хвост правого сегмента срезался у самой стены.
    // Запас в 2 колонки это покрывает (в сегменте максимум 2 таких символа с подсказками).
    let budget = width.saturating_sub(2);

    let mode_width = display_width(mode_label);
    let switch_width = display_width(switch) + 1; // пробел перед серым хоткеем
    let sep_width = 2;
    let min_gap = 2;

    // Левая часть до подсказок: режим, хоткей и разделитель. Её ширина от раскладки не зависит.
    let left_fixed = mode_width + switch_width + sep_width;

    // Правый сегмент ограничиваем доступным местом; не влезает — усекаем с «…», а не
    // молча теряем символ у края. Индикатор в эту ширину не вмешивается — слот на месте.
    let right_available = budget.saturating_sub(left_fixed + min_gap);
    let right_slot_width = right_slot_width.min(right_available);
    let right = truncate_display(right, right_slot_width);
    let right_width = display_width(&right);

    let free = budget.saturating_sub(left_fixed + right_slot_width + min_gap);

    // Приоритет: правый слот → git → подсказки. Индикатор забирает место раньше подсказок,
    // но не съедает их полностью: пока первый пункт подсказок не влезает целиком — уступает
    // индикатор, и футер ведёт себя ровно как без него.
    let hints_floor = display_width(first_hint(hints));
    let git_total = git
        .map(|git| display_width(git) + GIT_GAP)
        .filter(|total| free >= total + hints_floor)
        .unwrap_or(0);
    let git = if git_total > 0 {
        git.unwrap_or_default().to_string()
    } else {
        String::new()
    };

    let hints = truncate_display(hints, free.saturating_sub(git_total));
    let left_width = left_fixed + display_width(&hints);
    let gap = budget.saturating_sub(left_width + git_total + right_slot_width);
    let right_padding = right_slot_width.saturating_sub(right_width);

    FooterLayout {
        hints,
        git,
        gap,
        right,
        right_padding,
        right_slot_width,
    }
}

/// Первый пункт подсказок («? подсказки»): ниже него подсказки не ужимаются.
fn first_hint(hints: &str) -> &str {
    hints.split('·').next().unwrap_or(hints).trim_end()
}

/// Постоянный индикатор git-ref: имя ветки, а в detached HEAD — короткий SHA.
/// Без репозитория (или без ref-а) — `None`, индикатор не рисуется.
pub(crate) fn footer_git_segment(app: &App) -> Option<String> {
    let git_ref = app.git_ref.as_deref()?;
    Some(format!(
        "git: {}",
        truncate_display(git_ref, GIT_REF_MAX_COLS)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::footer::testkit::*;

    const MODE: &str = "чат";
    const SWITCH: &str = "shift+tab";
    const HINTS: &str = "? подсказки · / команды";
    const RIGHT: &str = "роли: Codex → Claude";

    #[test]
    fn git_segment_is_prefixed_and_capped_by_columns() {
        let mut app = bare_app();

        app.git_ref = None;
        assert_eq!(footer_git_segment(&app), None);

        app.git_ref = Some("main".to_string());
        assert_eq!(footer_git_segment(&app).as_deref(), Some("git: main"));

        // Длинная ветка усечена до GIT_REF_MAX_COLS (20): 19 символов и «…».
        app.git_ref = Some("feature/very-long-branch-name".to_string());
        assert_eq!(
            footer_git_segment(&app).as_deref(),
            Some("git: feature/very-long-b…")
        );
    }

    /// Пол подсказок — их ПЕРВЫЙ пункт, без хвостового пробела перед разделителем.
    #[test]
    fn first_hint_is_the_leading_item_without_trailing_space() {
        assert_eq!(first_hint(HINTS), "? подсказки");
        assert_eq!(first_hint("? shortcuts · / commands"), "? shortcuts");
        // Разделителя нет — пунктом считается вся строка.
        assert_eq!(first_hint("? подсказки"), "? подсказки");
    }

    fn layout(width: usize, git: Option<&str>) -> FooterLayout {
        footer_layout(width, MODE, SWITCH, HINTS, git, RIGHT, display_width(RIGHT))
    }

    /// Ровно та строка, которую соберёт draw_footer, — в колонках.
    fn rendered_width(l: &FooterLayout) -> usize {
        let git_gap = if l.git.is_empty() { 0 } else { GIT_GAP };
        display_width(MODE)
            + 1
            + display_width(SWITCH)
            + 2
            + display_width(&l.hints)
            + l.gap
            + display_width(&l.git)
            + git_gap
            + l.right_padding
            + display_width(&l.right)
    }

    /// Раскладка целиком, на руках посчитанная. budget = 98; слева 3 + 10 + 2 = 15,
    /// подсказки 23, индикатор 9 + 2 зазора, слот 20 → воздух ровно 29 колонок.
    #[test]
    fn layout_columns_are_pinned_to_exact_numbers() {
        let l = footer_layout(100, MODE, SWITCH, HINTS, Some("git: main"), RIGHT, 20);

        assert_eq!(
            l,
            FooterLayout {
                hints: HINTS.to_string(),
                git: "git: main".to_string(),
                gap: 29,
                right: RIGHT.to_string(),
                right_padding: 0,
                right_slot_width: 20,
            }
        );
    }

    /// Граница появления индикатора: он остаётся, пока рядом влезает первый пункт подсказок
    /// целиком (free == git + зазор + пол подсказок), и уходит на колонку раньше.
    #[test]
    fn git_appears_exactly_when_it_fits_next_to_the_first_hint() {
        // width 61 → free = 22 == (9 + 2) + 11.
        let fits = footer_layout(61, MODE, SWITCH, HINTS, Some("git: main"), RIGHT, 20);
        assert_eq!(fits.git, "git: main");
        assert_eq!(fits.hints, "? подсказк…"); // 11 колонок — ровно пол

        let tight = footer_layout(60, MODE, SWITCH, HINTS, Some("git: main"), RIGHT, 20);
        assert_eq!(tight.git, "");
        assert_eq!(tight.hints, "? подсказки · / кома…"); // 21 колонка: место индикатора отдано им
    }

    /// Узкий футер: слот не может занять свои 20 колонок и урезается ровно до того, что
    /// осталось от бюджета после левой части и минимального зазора. budget = 28,
    /// слева 3 + 10 + 2 = 15, зазор 2 → слоту достаётся ровно 11 колонок, и правый сегмент
    /// усечён по ним («…» внутри бюджета).
    #[test]
    fn narrow_footer_caps_the_right_slot_by_the_columns_left_over() {
        let l = layout(30, Some("git: main"));

        assert_eq!(l.right_slot_width, 11);
        assert_eq!(l.right, "роли: Code…");
        assert_eq!(l.right_padding, 0);
        assert_eq!(l.git, ""); // на индикатор места уже нет
        assert_eq!(rendered_width(&l), 30 - 2);
    }

    #[test]
    fn wide_footer_shows_git_and_fills_the_budget_exactly() {
        let l = layout(120, Some("git: main"));
        assert_eq!(l.git, "git: main");
        assert_eq!(l.hints, HINTS); // подсказки целы
        assert_eq!(l.right, RIGHT); // правый слот цел
        assert_eq!(rendered_width(&l), 120 - 2);
    }

    #[test]
    fn git_takes_room_before_hints_are_cut() {
        // Ширина, где полные подсказки + индикатор уже не помещаются: индикатор остаётся,
        // подсказки усекаются — но не исчезают.
        let l = layout(64, Some("git: main"));
        assert_eq!(l.git, "git: main");
        assert!(l.hints.ends_with('…') && display_width(&l.hints) < display_width(HINTS));
        assert!(l.hints.starts_with("? подсказки"));
        assert_eq!(l.right, RIGHT);
        assert_eq!(rendered_width(&l), 64 - 2);
    }

    #[test]
    fn narrow_footer_drops_git_and_keeps_todays_layout() {
        // Места нет даже под первый пункт подсказок рядом с индикатором — индикатор уходит,
        // а раскладка обязана совпасть с той, что была бы вообще без git.
        let with_git = layout(46, Some("git: main"));
        let without = layout(46, None);
        assert_eq!(with_git.git, "");
        assert_eq!(with_git, without);
    }

    #[test]
    fn long_branch_name_is_capped_in_columns() {
        // Обрезаем по КОЛОНКАМ: широкая (CJK) ветка иначе пробила бы бюджет вдвое.
        let long = "機能".repeat(20);
        assert!(display_width(&truncate_display(&long, GIT_REF_MAX_COLS)) <= GIT_REF_MAX_COLS);
        let ascii = "feature/very-long-branch-name-here";
        assert!(display_width(&truncate_display(ascii, GIT_REF_MAX_COLS)) <= GIT_REF_MAX_COLS);
    }

    #[test]
    fn layout_never_overflows_and_never_moves_the_right_slot() {
        let long_cjk = format!(
            "git: {}",
            truncate_display(&"機能".repeat(20), GIT_REF_MAX_COLS)
        );
        let variants: [Option<&str>; 3] = [None, Some("git: main"), Some(long_cjk.as_str())];

        for width in 20..=120usize {
            let baseline = layout(width, None);
            for git in variants {
                let l = layout(width, git);
                assert!(
                    rendered_width(&l) <= width.saturating_sub(2),
                    "width {width}: строка не влезает в бюджет"
                );
                assert_eq!(
                    l.right_slot_width, baseline.right_slot_width,
                    "width {width}: индикатор сдвинул правый слот"
                );
                assert_eq!(
                    l.right, baseline.right,
                    "width {width}: правый слот изменился"
                );
            }
        }
    }
}
