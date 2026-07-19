use super::*;

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
    use crate::ui::footer::testkit::*;

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

    /// Обёртка обязана ДЕЛЕГИРОВАТЬ разбор, а не решать сама.
    ///
    /// Прошлая версия утверждала конкретное значение (`assert!(!static_render())`) — и потому
    /// зависела от того, КТО её запустил. Супервайзер выставляет `CLAVE_STATIC_RENDER=1` на весь
    /// прогон (`cli.py`), так что под ним тест падал ЛОЖНО: проверки краснели, петля не сходилась,
    /// зрение и мутационный гейт не запускались вовсе. Тест, который ломается от собственного
    /// инструмента, хуже отсутствия теста — он не находит дефект, а выдумывает его.
    ///
    /// Сравнение вместо утверждения лечит это начисто: обёртка обязана вернуть ровно то, что даёт
    /// чистый разбор ТЕКУЩЕГО значения, каким бы оно ни было. Переменную по-прежнему не трогаем —
    /// только читаем, поэтому гонки с параллельными тестами нет.
    ///
    /// Каким бы ни было окружение — обёртка возвращает то же, что чистый разбор его значения.
    #[test]
    fn the_wrapper_only_delegates_to_the_parser() {
        let value = std::env::var("CLAVE_STATIC_RENDER").ok();

        assert_eq!(static_render(), static_render_from(value.as_deref()));
    }

    /// Флаг обязан ВКЛЮЧАТЬ режим и обязан его НЕ включать — оба состояния, в своём процессе.
    ///
    /// Это снимает последнюю границу покрытия в футере. Мутанты «static_render всегда true» и
    /// «всегда false» жили тут потому, что увидеть обе ветки можно, только МЕНЯЯ переменную, — а
    /// менять её в тесте нельзя. И дело не только в футере: `setenv` и `getenv` не потокобезопасны
    /// между собой в принципе, а окружение параллельно читает и сам std (temp_dir, HOME). Мутация
    /// env под работающим `cargo test` — гонка со ВСЕМ процессом, а не с одним тестом.
    ///
    /// Поэтому тест перезапускает СЕБЯ в дочернем процессе, где окружение задано снаружи и никто
    /// его не трогает: там он один. Продукт при этом не меняется ни на строку — лечится тест, а не
    /// код под ним.
    #[test]
    fn the_env_flag_switches_the_mode_both_ways() {
        match std::env::var(CASE).ok().as_deref() {
            // Мы — дочерний процесс: окружение уже выставлено, просто проверяем.
            Some("on") => assert!(
                static_render(),
                "с CLAVE_STATIC_RENDER=1 режим обязан включиться"
            ),
            Some("off") => assert!(
                !static_render(),
                "без флага футер обязан жить, а не замереть"
            ),
            // Мы — родитель: гоняем оба случая как отдельные процессы.
            _ => {
                run_case_in_child("on", Some("1"));
                run_case_in_child("off", None);
            }
        }
    }

    /// Имя переменной, которой родитель говорит ребёнку, что именно проверять.
    const CASE: &str = "CLAVE_TEST_STATIC_RENDER_CASE";
    /// Полный путь теста — им фильтруется дочерний прогон.
    const SELF: &str = "ui::footer::slot::tests::the_env_flag_switches_the_mode_both_ways";

    fn run_case_in_child(case: &str, flag: Option<&str>) {
        let exe = std::env::current_exe().expect("путь к тестовому бинарю");
        let mut cmd = std::process::Command::new(exe);
        cmd.args([SELF, "--exact", "--nocapture"])
            .env(CASE, case)
            .env_remove("CLAVE_STATIC_RENDER");
        if let Some(value) = flag {
            cmd.env("CLAVE_STATIC_RENDER", value);
        }

        let out = cmd.output().expect("дочерний тест не запустился");
        let stdout = String::from_utf8_lossy(&out.stdout);

        assert!(
            out.status.success(),
            "случай «{case}» провалился:\n{stdout}{}",
            String::from_utf8_lossy(&out.stderr),
        );
        // Коду возврата тут верить НЕЛЬЗЯ, и это не паранойя: с опечаткой в фильтре ребёнок гоняет
        // НОЛЬ тестов и выходит нулём — «0 passed; 156 filtered out», успех. Первая версия этого
        // теста ровно так и зеленела, ничего не проверяя. Требуем предъявить прогнанное.
        assert!(
            stdout.contains("1 passed"),
            "дочерний прогон не состоялся (случай «{case}»): фильтр не нашёл тест, а ноль тестов \
             читаются как успех. Вывод:\n{stdout}"
        );
    }

    /// Что бы ни показывал слот (вращение или статичный режим) — это ОДИН из сегментов
    /// приложения, а не пустота и не посторонняя строка.
    #[test]
    fn footer_right_target_is_one_of_the_app_segments() {
        let app = bare_app();
        let target = footer_right_target(&app);

        assert!(!target.is_empty());
        assert!(
            footer_right_segments(&app).contains(&target),
            "правый слот показывает не свой сегмент: {target:?}"
        );
    }

    /// Приложение с ФИКСИРОВАННЫМИ полями, которые читает `footer_right_segments`: `App::new()`
    /// берёт язык, тему и усилия из конфига на диске, и без этого тест зависел бы от машины.
    /// Переменные окружения не трогаем — соседние тесты в этом же процессе рендерят футер.
    fn segments_app() -> App {
        let mut app = bare_app();
        app.lang = Language::Ru;
        app.status = "готов".to_string();
        app.mode = Mode::CodexClaude;
        app.direct_provider = Provider::Claude;
        app.theme = Theme::Amber;
        app.linked_effort_split = true;
        app.claude_effort_index = 4; // max
        app.codex_effort_index = 3; // xhigh
        app.usage = SessionUsage::new();
        app
    }

    /// Покой: статус равен «готов», токенов не потрачено. Сегментов ровно четыре — сравниваем
    /// СПИСОК ЦЕЛИКОМ, поэтому лишний сегмент здесь так же виден, как пропавший.
    #[test]
    fn idle_footer_hides_status_and_usage_and_shows_the_two_role_models() {
        let app = segments_app();

        assert_eq!(
            footer_right_segments(&app),
            [
                "роли: Codex → Claude",
                "чат: Claude",
                "тема: Amber",
                "усилие: Claude max · Codex xhigh",
            ]
        );
    }

    /// Работа: статус сменился, роли схлопнулись в одну модель, расход появился. Это вторые
    /// ветки всех трёх решений функции — без них половина подмен в условиях остаётся незаметной.
    #[test]
    fn busy_footer_adds_status_and_usage_and_folds_equal_roles() {
        let mut app = segments_app();
        app.status = "думает…".to_string();
        app.mode = Mode::ClaudeOnly; // архитектор и ревьюер — одна модель
        app.usage.record(
            Provider::Claude,
            RunUsage {
                input: 1200,
                cost_usd: 0.045,
                ..Default::default()
            },
        );

        assert_eq!(
            footer_right_segments(&app),
            [
                "статус: думает…",
                "роли: Claude",
                "чат: Claude",
                "тема: Amber",
                "усилие: max",
                "расход: 1.2k · $0.045",
            ]
        );
    }

    /// Подписи сегментов — переводимые: у английского пользователя футер обязан быть английским.
    #[test]
    fn english_footer_uses_english_keys() {
        let mut app = segments_app();
        app.lang = Language::En;
        app.status = "thinking…".to_string();

        assert_eq!(
            footer_right_segments(&app),
            [
                "status: thinking…",
                "roles: Codex → Claude",
                "chat: Claude",
                "theme: Amber",
                "effort: Claude max · Codex xhigh",
            ]
        );
    }

    /// Английское «готов» — это `ready`, и статус-сегмент прячется именно по нему, а не по
    /// русскому слову: иначе у английского пользователя футер вечно кричал бы «status: ready».
    #[test]
    fn english_idle_status_is_hidden_by_the_english_word() {
        let mut app = segments_app();
        app.lang = Language::En;
        app.status = "ready".to_string();

        assert_eq!(
            footer_right_segments(&app).first().map(String::as_str),
            Some("roles: Codex → Claude")
        );
    }

    #[test]
    fn without_static_render_the_slot_still_rotates() {
        let picked = pick_right_segment(segments(), false);

        assert!(
            segments().contains(&picked),
            "вращение должно давать один из сегментов"
        );
    }

    /// Лестница перехода — чистая таблица, и проверять её надо целиком, точными цветами.
    /// Момент `1` мс обязан быть в списке: только на нём видно, что шаг СЧИТАЕТСЯ делением.
    /// Делить на 90 — шаг 0, умножать — шаг 4, брать остаток — шаг 1; три разных цвета.
    /// На нуле все три варианта дают шаг 0 и подмену не поймать — одного `0` в таблице мало.
    /// Границы 89/90, 179/180, 269/270 держат ширину ступени ровно в 90 мс,
    /// а 10_000 — клэмп `.min(4)`: без него это выход за край палитры, то есть паника.
    #[test]
    fn transition_color_walks_the_ramp_in_both_directions() {
        fn ramp(entering: bool) -> Vec<Color> {
            [0, 1, 89, 90, 179, 180, 269, 270, 359, 360, 10_000]
                .iter()
                .map(|&ms| footer_transition_color(Theme::Amber, ms, entering))
                .collect()
        }

        let dim = Color::Indexed(136);
        let soft = Color::Indexed(229);

        assert_eq!(
            ramp(true),
            vec![
                dim,
                dim,
                dim,
                Color::DarkGray,
                Color::DarkGray,
                Color::Gray,
                Color::Gray,
                soft,
                soft,
                soft,
                soft,
            ],
            "появляющийся текст идёт от accent_dim к accent_soft"
        );

        assert_eq!(
            ramp(false),
            vec![
                soft,
                soft,
                soft,
                Color::Gray,
                Color::Gray,
                Color::DarkGray,
                Color::DarkGray,
                dim,
                dim,
                dim,
                dim,
            ],
            "гаснущий текст идёт от accent_soft к accent_dim"
        );
    }

    /// Часы у перехода настоящие (`Instant::elapsed`), поэтому «состаренный» момент отсчёта
    /// сам по себе ничего не гарантирует: планировщик может вытеснить поток между созданием
    /// `Instant` и вызовом. Берём замер в скобки — `before`/`after` вокруг вызова; из-за
    /// монотонности `Instant` попадание обеих границ в окно означает, что и внутри вызова
    /// elapsed был в нём же. Промах не роняет тест, а просто отбрасывает попытку.
    fn sample_segment(age_ms: u64, lo: u128, hi: u128) -> (String, Style) {
        for _ in 0..1000 {
            let mut app = segments_app();
            app.footer_right_text = "чат: Claude".to_string();
            app.footer_right_previous_text = Some("тема: Amber".to_string());
            let changed_at = Instant::now() - Duration::from_millis(age_ms);
            app.footer_right_changed_at = Some(changed_at);

            let before = changed_at.elapsed().as_millis();
            let segment = footer_right_segment(&app);
            let after = changed_at.elapsed().as_millis();

            if before >= lo && after < hi {
                return segment;
            }
            std::thread::yield_now();
        }
        panic!("не удалось попасть в окно {lo}..{hi} мс за 1000 попыток");
    }

    /// До границы показан ПРЕДЫДУЩИЙ текст и он гаснет. Замена `<` на `==` или `>` уводит
    /// сюда else-ветку — вместо «тема: Amber» вылезет «чат: Claude».
    #[test]
    fn before_the_switch_the_previous_text_is_still_fading_out() {
        let (text, style) = sample_segment(135, 90, 180);

        assert_eq!(text, "тема: Amber");
        assert_eq!(style, Style::default().fg(Color::Gray));
    }

    /// Ровно 360 мс — единственная точка, где `<` и `<=` расходятся: оригинал уже переключился
    /// на новый текст. Цвет тут у обеих веток один и тот же (`fade[4]` и `entering[0]` — это
    /// accent_dim), поэтому мутанта держит именно ТЕКСТ, стиль — второй якорь.
    #[test]
    fn exactly_at_the_boundary_the_new_text_is_already_shown() {
        let (text, style) = sample_segment(360, 360, 361);

        assert_eq!(text, "чат: Claude");
        assert_eq!(style, Style::default().fg(Color::Indexed(136)));
    }

    /// После границы шаг лестницы считается ВЫЧИТАНИЕМ: 495 - 360 = 135 → шаг 1 → DarkGray.
    /// Сложение дало бы 855 → клэмп 4 → accent_soft(229), деление — 1 → шаг 0 → accent_dim(136).
    /// Три разных цвета, поэтому обе подмены ловятся одним сравнением стиля.
    #[test]
    fn after_the_switch_the_new_text_fades_in_from_the_subtracted_offset() {
        let (text, style) = sample_segment(495, 450, 540);

        assert_eq!(text, "чат: Claude");
        assert_eq!(style, Style::default().fg(Color::DarkGray));
    }
}
