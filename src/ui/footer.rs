use super::*;

/// Максимум колонок под имя ветки: длинная `feature/...` иначе вытеснила бы индикатор
/// с экрана целиком уже на 100 колонках.
const GIT_REF_MAX_COLS: usize = 20;
/// Разделитель между git-индикатором и вращающимся правым слотом.
const GIT_GAP: usize = 2;

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

pub(crate) fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.height == 0 {
        return;
    }

    let git = footer_git_segment(app);

    if let Some((message, shown_at)) = &app.footer_notice {
        if shown_at.elapsed() <= Duration::from_secs(2) {
            draw_footer_notice(frame, area, app, message, git.as_deref());
            return;
        }
    }

    let mode_label = app.chat_mode.label(app.lang);
    let switch = MODE_SWITCH_KEYS;
    let hints = app
        .lang
        .choose("? подсказки · / команды", "? shortcuts · / commands");
    let (right, right_style) = footer_right_segment(app);

    let layout = footer_layout(
        area.width as usize,
        mode_label,
        switch,
        hints,
        git.as_deref(),
        &right,
        footer_right_slot_width(app),
    );

    // Разделитель перед слотом рисуем, только когда индикатор показан: его ширина уже
    // заложена в бюджет раскладки (GIT_GAP), без спана git прилип бы к правому сегменту.
    let git_gap = if layout.git.is_empty() { 0 } else { GIT_GAP };
    let line = Line::from(vec![
        Span::styled(
            mode_label,
            Style::default()
                .fg(app.chat_mode.color())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(switch, Style::default().fg(MUTED)),
        Span::raw("  "),
        Span::styled(layout.hints, Style::default().fg(app.theme.accent_soft())),
        Span::raw(" ".repeat(layout.gap)),
        Span::styled(layout.git, Style::default().fg(MUTED)),
        Span::raw(" ".repeat(git_gap + layout.right_padding)),
        Span::styled(layout.right, right_style),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

/// Уведомление занимает футер целиком на 2 секунды — но индикатор постоянный, поэтому
/// он остаётся у правого края и здесь (если помещается рядом с текстом уведомления).
fn draw_footer_notice(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    message: &str,
    git: Option<&str>,
) {
    let budget = (area.width as usize).saturating_sub(2);
    let notice_style = Style::default()
        .fg(app.theme.accent_soft())
        .add_modifier(Modifier::BOLD);

    let git_total = git
        .map(|git| display_width(git) + GIT_GAP)
        .filter(|total| budget > *total)
        .unwrap_or(0);
    if git_total == 0 {
        let text = truncate_display(message, area.width as usize);
        frame.render_widget(Paragraph::new(text).style(notice_style), area);
        return;
    }

    let message = truncate_display(message, budget - git_total);
    let gap = budget.saturating_sub(display_width(&message) + git_total);
    let line = Line::from(vec![
        Span::styled(message, notice_style),
        Span::raw(" ".repeat(gap + GIT_GAP)),
        Span::styled(
            git.unwrap_or_default().to_string(),
            Style::default().fg(MUTED),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

pub(crate) fn footer_right_segments(app: &App) -> Vec<String> {
    let lang = app.lang;
    let ready = lang.choose("готов", "ready");
    let mut segments = Vec::new();

    if app.status != ready {
        segments.push(format!(
            "{}: {}",
            lang.choose("статус", "status"),
            app.status
        ));
    }

    // Роли планирования: архитектор → ревьюер (одна модель, если совпадают). Отдельный
    // сегмент «mode» убран — он дублировал ровно эти же роли.
    let architect = app.mode.architect_provider();
    let reviewer = app.mode.reviewer_provider();
    let roles = if architect == reviewer {
        architect.title().to_string()
    } else {
        format!("{} → {}", architect.title(), reviewer.title())
    };
    segments.push(format!("{}: {}", lang.choose("роли", "roles"), roles));
    segments.push(format!(
        "{}: {}",
        lang.choose("чат", "chat"),
        app.direct_provider.title()
    ));
    segments.push(format!(
        "{}: {}",
        lang.choose("тема", "theme"),
        app.theme.title()
    ));
    segments.push(format!(
        "{}: {}",
        lang.choose("усилие", "effort"),
        app.human_effort_summary()
    ));
    if app.usage.total_tokens() > 0 {
        segments.push(format!(
            "{}: {} · ${:.3}",
            lang.choose("расход", "usage"),
            format_token_count(app.usage.total_tokens() as usize),
            app.usage.total_cost_usd()
        ));
    }
    segments
}

/// Разбор значения флага — чистая функция: её и проверяет тест. Если бы тест крутил саму
/// переменную окружения, он менял бы `static_render()` под ногами у любого параллельного теста
/// в этом же процессе, который в этот момент рендерит футер.
fn static_render_from(value: Option<&str>) -> bool {
    value == Some("1")
}

/// Рендер без зависимости от стенных часов — для внешнего наблюдения (снимок экрана и сравнение
/// с эталоном). Включается `CLAVE_STATIC_RENDER=1`, как и `CLAVE_SKIP_ONBOARDING`.
///
/// ГРАНИЦА ПОКРЫТИЯ, и она честная. Здесь одна строка — чтение глобального состояния процесса.
/// Мутационный прогон оставляет тут двух выживших («всегда true», «всегда false»), и убить обоих
/// нельзя: для этого тест должен увидеть переменную и в одном значении, и в другом, то есть
/// МЕНЯТЬ её — под ногами у параллельных тестов, которые в этот момент рендерят футер. Ровно та
/// гонка, ради устранения которой разбор и вынесен в чистую `static_render_from`.
///
/// Поэтому граница сжата до минимума (одна строка) и объявлена вслух, а не замаскирована
/// тестом-декорацией. Опасного из двух мутантов — «всегда true» — тест ниже всё же убивает:
/// у пользователя без флага футер обязан жить, а не замереть.
fn static_render() -> bool {
    static_render_from(std::env::var("CLAVE_STATIC_RENDER").ok().as_deref())
}

/// Что показать в правом слоте футера.
///
/// Слот вращается по стенным часам: `(unix_secs / 8) % N`. Значит два снимка ОДНОГО И ТОГО ЖЕ
/// кода, сделанные в разные секунды, показывают разные сегменты разной ширины — и всякое
/// сравнение рендеров превращается в подбрасывание монеты. На этом сорвался живой прогон:
/// эталон поймал «чат: Claude», проверяемая сборка — «усилие: Claude max · Codex xhigh», и
/// разница была объявлена регрессией, которой никто не вносил.
///
/// В статичном режиме берём самый ШИРОКИЙ сегмент, а не первый: так рендер и детерминирован, и
/// показывает худший случай по ширине — тот самый, на котором обрезка у правой границы и ловится.
pub(crate) fn pick_right_segment(segments: Vec<String>, static_mode: bool) -> String {
    if static_mode {
        return segments
            .into_iter()
            .max_by_key(|segment| display_width(segment))
            .unwrap_or_default();
    }

    let phase = rotating_phase(8, segments.len());
    segments.get(phase).cloned().unwrap_or_default()
}

pub(crate) fn footer_right_target(app: &App) -> String {
    pick_right_segment(footer_right_segments(app), static_render())
}

pub(crate) fn footer_right_slot_width(app: &App) -> usize {
    let current_width = display_width(&app.footer_right_text);
    let previous_width = app
        .footer_right_previous_text
        .as_ref()
        .map(|previous| display_width(previous))
        .unwrap_or(0);

    current_width.max(previous_width)
}

pub(crate) fn footer_right_segment(app: &App) -> (String, Style) {
    let base_style = Style::default().fg(app.theme.accent_soft());
    let Some(changed_at) = app.footer_right_changed_at else {
        return (app.footer_right_text.clone(), base_style);
    };

    let elapsed_ms = changed_at.elapsed().as_millis();
    let previous = app
        .footer_right_previous_text
        .as_ref()
        .unwrap_or(&app.footer_right_text);

    if elapsed_ms < 360 {
        (
            previous.clone(),
            Style::default().fg(footer_transition_color(app.theme, elapsed_ms, false)),
        )
    } else {
        (
            app.footer_right_text.clone(),
            Style::default().fg(footer_transition_color(app.theme, elapsed_ms - 360, true)),
        )
    }
}

pub(crate) fn footer_transition_color(theme: Theme, elapsed_ms: u128, entering: bool) -> Color {
    let step = (elapsed_ms / 90).min(4) as usize;
    let palette = if entering {
        [
            theme.accent_dim(),
            Color::DarkGray,
            Color::Gray,
            theme.accent_soft(),
            theme.accent_soft(),
        ]
    } else {
        [
            theme.accent_soft(),
            Color::Gray,
            Color::DarkGray,
            theme.accent_dim(),
            theme.accent_dim(),
        ]
    };
    palette[step]
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODE: &str = "чат";
    const SWITCH: &str = "shift+tab";
    const HINTS: &str = "? подсказки · / команды";
    const RIGHT: &str = "роли: Codex → Claude";

    fn segments() -> Vec<String> {
        [
            "чат: Claude",
            "тема: Amber",
            "усилие: Claude max · Codex xhigh",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    /// Правый слот вращается по стенным часам. Значит внешнее наблюдение (снимок экрана и
    /// сравнение с эталоном) видит РАЗНЫЕ сегменты разной ширины у одного и того же кода — и
    /// объявляет регрессией разницу, которой никто не вносил. Ровно так и случилось на живом
    /// прогоне. Статичный режим убирает часы из рендера.
    #[test]
    fn static_render_pins_the_slot_and_shows_the_widest_segment() {
        let picked = pick_right_segment(segments(), true);

        assert_eq!(picked, "усилие: Claude max · Codex xhigh");
        // Детерминированность — суть режима: повтор обязан дать то же самое.
        assert_eq!(pick_right_segment(segments(), true), picked);
    }

    /// Широчайший сегмент выбран не из вкуса: он и есть худший случай по ширине, а обрезка у
    /// правой границы ловится именно на нём.
    #[test]
    fn the_pinned_segment_is_the_widest_by_columns_not_by_chars() {
        let widest = pick_right_segment(segments(), true);

        let max_cols = segments().iter().map(|s| display_width(s)).max().unwrap();
        assert_eq!(display_width(&widest), max_cols);
    }

    /// Режим включает значение флага — и ровно `1`. Проверяем чистый разбор: `set_var` здесь
    /// менял бы флаг под ногами у параллельных тестов, которые рендерят футер в этом же процессе.
    #[test]
    fn static_render_is_switched_only_by_the_value_one() {
        assert!(static_render_from(Some("1")));

        assert!(!static_render_from(Some("0")));
        assert!(!static_render_from(Some("")));
        assert!(!static_render_from(Some("true")));
        assert!(!static_render_from(None)); // переменной нет
    }

    /// Без флага футер обязан ЖИТЬ. Это про пользователя, а не про инструмент: статичный режим,
    /// включившийся сам, замораживает правый слот у всех.
    ///
    /// Переменную намеренно НЕ трогаем — только читаем в состоянии «её нет». Так тест убивает
    /// мутанта «static_render всегда true», не заводя гонку с параллельными тестами. Второго
    /// мутанта («всегда false») отсюда убить нечем: для этого пришлось бы переменную ВЫСТАВИТЬ.
    /// Эта граница объявлена вслух в комментарии к `static_render` — маскировать её тестом,
    /// который ничего не доказывает, было бы хуже, чем признать.
    #[test]
    fn without_the_env_flag_the_footer_keeps_rotating() {
        assert!(!static_render());
    }

    /// Что бы ни показывал слот (вращение или статичный режим) — это ОДИН из сегментов
    /// приложения, а не пустота и не посторонняя строка.
    #[test]
    fn footer_right_target_is_one_of_the_app_segments() {
        let app = App::new();
        let target = footer_right_target(&app);

        assert!(!target.is_empty());
        assert!(
            footer_right_segments(&app).contains(&target),
            "правый слот показывает не свой сегмент: {target:?}"
        );
    }

    #[test]
    fn git_segment_is_prefixed_and_capped_by_columns() {
        let mut app = App::new();

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

    #[test]
    fn without_static_render_the_slot_still_rotates() {
        let picked = pick_right_segment(segments(), false);

        assert!(
            segments().contains(&picked),
            "вращение должно давать один из сегментов"
        );
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
