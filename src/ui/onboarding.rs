use super::*;

pub(crate) fn draw_onboarding(frame: &mut Frame<'_>, area: Rect, app: &App) {
    frame.render_widget(Clear, area);
    let Some(onboarding) = app.onboarding.as_ref() else {
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(1)])
        .split(area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(vec![
            Span::styled(
                format!(" {APP_NAME} Setup "),
                Style::default()
                    .fg(app.theme.accent())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("first run ", Style::default().fg(MUTED)),
        ]))
        .border_style(Style::default().fg(app.theme.accent()));
    frame.render_widget(block, chunks[0]);

    let inner = Rect {
        x: chunks[0].x + 2,
        y: chunks[0].y + 1,
        width: chunks[0].width.saturating_sub(4),
        height: chunks[0].height.saturating_sub(2),
    };

    let lines = match onboarding.step {
        OnboardingStep::Provider => onboarding_provider_lines(app, onboarding),
        OnboardingStep::Auth => onboarding_auth_lines(app, onboarding),
        OnboardingStep::Settings => onboarding_settings_lines(app, onboarding),
    };

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    draw_footer(frame, chunks[1], app);
}

pub(crate) fn onboarding_provider_lines(app: &App, onboarding: &Onboarding) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::styled(
            app.lang
                .choose("Выбор связки моделей", "Choose model pairing"),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(onboarding.message.clone()),
        Line::from(""),
    ];

    for index in 0..provider_count() {
        let selected = index == onboarding.provider_index;
        let mode = provider_mode(index);
        let style = if selected {
            Style::default()
                .fg(Color::White)
                .bg(app.theme.accent_bg())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.accent_soft())
        };
        lines.push(Line::from(vec![
            Span::styled(
                if selected { "› " } else { "  " },
                Style::default().fg(app.theme.accent()),
            ),
            Span::styled(format!("{:<14}", mode.as_str()), style),
            Span::raw(" "),
            Span::styled(
                provider_description(mode, app.lang),
                Style::default().fg(MUTED),
            ),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::styled(
        app.lang.choose(
            "↑/↓ выбрать · Enter продолжить · Ctrl+C выйти",
            "↑/↓ choose · Enter continue · Ctrl+C exit",
        ),
        Style::default().fg(MUTED),
    ));
    lines
}

pub(crate) fn onboarding_auth_lines(app: &App, onboarding: &Onboarding) -> Vec<Line<'static>> {
    let codex_needed = app.mode.needs_codex();
    let claude_needed = app.mode.needs_claude();
    vec![
        Line::styled(
            app.lang.choose("Авторизация CLI", "CLI authentication"),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Line::from(onboarding.message.clone()),
        Line::from(""),
        auth_status_line(
            "Codex",
            codex_needed,
            onboarding.codex_installed,
            onboarding.codex_authenticated,
            &onboarding.codex_status,
            "codex login",
            "C",
            app.lang,
            app.theme,
        ),
        auth_status_line(
            "Claude",
            claude_needed,
            onboarding.claude_installed,
            onboarding.claude_authenticated,
            &onboarding.claude_status,
            "claude auth login",
            "L",
            app.lang,
            app.theme,
        ),
        Line::from(""),
        Line::styled(
            app.lang.choose(
                "C запустить Codex login · L запустить Claude auth login · Enter дальше · Esc назад",
                "C run Codex login · L run Claude auth login · Enter next · Esc back",
            ),
            Style::default().fg(MUTED),
        ),
    ]
}

pub(crate) fn onboarding_settings_lines(app: &App, onboarding: &Onboarding) -> Vec<Line<'static>> {
    let rows = [
        (
            app.lang.choose("Раунды ревью", "Review rounds").to_string(),
            app.rounds.to_string(),
        ),
        ("Effort".to_string(), app.effort_summary()),
        (
            app.lang.choose("Язык", "Language").to_string(),
            app.lang.as_str().to_string(),
        ),
    ];

    let mut lines = vec![
        Line::styled(
            app.lang.choose("Стартовые настройки", "Startup settings"),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(onboarding.message.clone()),
        Line::from(""),
    ];

    for (index, (label, value)) in rows.into_iter().enumerate() {
        let selected = index == onboarding.setting_index;
        let style = if selected {
            Style::default()
                .fg(Color::White)
                .bg(app.theme.accent_bg())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.accent_soft())
        };
        lines.push(Line::from(vec![
            Span::styled(
                if selected { "› " } else { "  " },
                Style::default().fg(app.theme.accent()),
            ),
            Span::styled(format!("{label:<18}"), style),
            Span::raw(" "),
            Span::styled(value, Style::default().fg(Color::White)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            app.lang.choose("Режим ", "Mode "),
            Style::default().fg(MUTED),
        ),
        Span::styled(
            app.mode.as_str(),
            Style::default()
                .fg(app.theme.accent_soft())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.lang.choose(" · Артефакты ", " · Artifacts "),
            Style::default().fg(MUTED),
        ),
        Span::styled(
            app.out_dir.clone(),
            Style::default().fg(app.theme.accent_soft()),
        ),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::styled(
        app.lang.choose(
            "↑/↓ поле · ←/→ изменить · L язык · Enter сохранить · Esc назад",
            "↑/↓ field · ←/→ change · L language · Enter save · Esc back",
        ),
        Style::default().fg(MUTED),
    ));
    lines
}

// Связный набор данных строки статуса авторизации; дробить ради порога lint не нужно.
#[allow(clippy::too_many_arguments)]
pub(crate) fn auth_status_line(
    name: &'static str,
    needed: bool,
    installed: bool,
    authenticated: bool,
    status_text: &str,
    command: &'static str,
    key: &'static str,
    lang: Language,
    theme: Theme,
) -> Line<'static> {
    let need_label = if needed {
        lang.choose("нужен", "needed")
    } else {
        lang.choose("опционально", "optional")
    };
    let status = if !installed {
        lang.choose("CLI не найден", "CLI missing").to_string()
    } else if authenticated {
        lang.choose("аккаунт готов", "account ready").to_string()
    } else {
        lang.choose("не авторизован", "not logged in").to_string()
    };
    let status_style = if installed && authenticated {
        Style::default()
            .fg(theme.accent_soft())
            .add_modifier(Modifier::BOLD)
    } else if installed {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    };
    let detail = truncate_chars(status_text, 36);

    Line::from(vec![
        Span::styled(
            format!("{name:<8}"),
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{need_label:<12}"), Style::default().fg(MUTED)),
        Span::styled(status, status_style),
        Span::raw(" · "),
        Span::styled(format!("{key}: {command}"), Style::default().fg(MUTED)),
        Span::raw(" · "),
        Span::styled(detail, Style::default().fg(Color::DarkGray)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, buffer::Buffer};

    // ── Хелперы (по образцу src/ui/effort.rs::mod tests) ─────────────────────

    fn onboarding_app() -> App {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = std::env::temp_dir().join(format!(
            "clave-onboarding-{}-{}",
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
        // Ожидания расписаны на английских строках.
        app.lang = Language::En;
        app.theme = Theme::Purple;
        app
    }

    /// Онбординг собираем ЛИТЕРАЛОМ — `Onboarding::new` дёргает реальные auth-пробы
    /// (codex/claude в PATH), которые виснут и зеленеют локально, но падают на CI.
    fn onboarding(step: OnboardingStep) -> Onboarding {
        Onboarding {
            step,
            provider_index: 0,
            setting_index: 0,
            codex_installed: true,
            claude_installed: true,
            codex_authenticated: true,
            claude_authenticated: true,
            codex_status: String::new(),
            claude_status: String::new(),
            message: "msg".to_string(),
        }
    }

    fn screen(app: &App, width: u16, height: u16) -> Buffer {
        let mut terminal =
            Terminal::new(TestBackend::new(width, height)).expect("оффскрин-терминал");
        terminal
            .draw(|frame| draw_onboarding(frame, frame.area(), app))
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

    fn joined(lines: &[Line<'_>]) -> String {
        lines.iter().map(line_text).collect::<Vec<_>>().join("\n")
    }

    // ── Часть 1. draw_onboarding рисует контент внутри рамки ──────────────────

    /// Экран первого запуска обязан отрисоваться и попасть В КАДР: заголовок шага
    /// внутри рамки, не наезжая ни на левый бордюр, ни на верхнюю строку с заголовком.
    /// Рендер идёт в угол (0,0) — тогда неверный сдвиг inner уходит в u16-underflow.
    #[test]
    fn onboarding_screen_renders_the_step_inside_the_frame() {
        let mut app = onboarding_app();
        app.onboarding = Some(onboarding(OnboardingStep::Provider));

        // Сам факт, что screen() не паникует при area (0,0), ловит 29:24 `+→-`
        // и 30:24 `+→-`: у них inner.x/inner.y уходят в u16-underflow (overflow-checks).
        let rows = buffer_rows(&screen(&app, 60, 14));

        // 4:5 `тело → ()`: без тела экран пуст, заголовок шага исчезает.
        let title_row = rows
            .iter()
            .find(|row| row.contains("Choose model pairing"))
            .unwrap_or_else(|| {
                panic!(
                    "на экране нет заголовка шага Provider:\n{}",
                    rows.join("\n")
                )
            });

        // 29:24 `+→*` (inner.x: 2→0): контент наезжает на левый бордюр.
        assert!(
            title_row.starts_with('│'),
            "контент наехал на левую рамку: {title_row:?}"
        );

        // 30:24 `+→*` (inner.y: 1→0): контент затирает верхнюю рамку с заголовком.
        assert!(
            rows[0].contains("Setup"),
            "контент затёр заголовок рамки: {:?}",
            rows[0]
        );
    }

    // ── Часть 2. onboarding_provider_lines ───────────────────────────────────

    /// Список связок непустой и осмысленный, а маркер `›` стоит ровно на выбранной
    /// строке — и только на ней.
    #[test]
    fn provider_lines_list_providers_and_mark_the_selected_one() {
        let app = onboarding_app();
        let onb = Onboarding {
            provider_index: 1,
            ..onboarding(OnboardingStep::Provider)
        };
        let lines = onboarding_provider_lines(&app, &onb);

        // 46:5 `→ vec![]` и `→ vec![Default::default()]`: осмысленный непустой контент.
        let text = joined(&lines);
        assert!(text.contains("Choose model pairing"), "нет заголовка шага");
        assert!(text.contains("choose"), "нет подсказки навигации");
        assert!(
            text.contains("Claude drafts, Codex reviews"),
            "нет описания провайдера"
        );

        // 59:30 `== → !=`: провайдер-строки идут после 3 шапочных.
        let provider_lines = &lines[3..3 + provider_count()];
        let marked: Vec<usize> = provider_lines
            .iter()
            .enumerate()
            .filter(|(_, line)| span_texts(line)[0] == "› ")
            .map(|(index, _)| index)
            .collect();
        // Ровно одна помечена (а не три), и это именно provider_index.
        assert_eq!(marked, vec![1], "маркер обязан стоять на выбранной строке");
    }

    // ── Часть 3. onboarding_auth_lines ───────────────────────────────────────

    /// Экран авторизации показывает заголовок и обе строки CLI.
    #[test]
    fn auth_lines_show_the_header_and_both_clis() {
        let app = onboarding_app();
        let onb = onboarding(OnboardingStep::Auth);
        let lines = onboarding_auth_lines(&app, &onb);

        // 95:5 `→ vec![]` и `→ vec![Default::default()]`: осмысленный непустой контент.
        let text = joined(&lines);
        assert!(text.contains("CLI authentication"), "нет заголовка");
        assert!(text.contains("Codex"), "нет строки Codex");
        assert!(text.contains("Claude"), "нет строки Claude");
    }

    // ── Часть 4. onboarding_settings_lines ───────────────────────────────────

    /// Экран настроек показывает все поля и метку режима, а маркер стоит ровно на
    /// выбранном поле.
    #[test]
    fn settings_lines_show_all_rows_and_mark_the_selected_one() {
        let app = onboarding_app();
        let onb = Onboarding {
            setting_index: 1,
            ..onboarding(OnboardingStep::Settings)
        };
        let lines = onboarding_settings_lines(&app, &onb);

        // 138:5 `→ vec![]` и `→ vec![Default::default()]`: осмысленный непустой контент.
        let text = joined(&lines);
        assert!(text.contains("Startup settings"), "нет заголовка");
        assert!(text.contains("Review rounds"), "нет поля раундов");
        assert!(text.contains("Effort"), "нет поля effort");
        assert!(text.contains("Language"), "нет поля языка");
        assert!(text.contains("Mode"), "нет метки режима");

        // 162:30 `== → !=`: строки настроек идут после 3 шапочных.
        let setting_lines = &lines[3..6];
        let marked: Vec<usize> = setting_lines
            .iter()
            .enumerate()
            .filter(|(_, line)| span_texts(line)[0] == "› ")
            .map(|(index, _)| index)
            .collect();
        // Ровно одна помечена (а не две), и это строка «Effort».
        assert_eq!(marked, vec![1], "маркер обязан стоять на выбранном поле");
    }

    // ── Часть 5. auth_status_line ────────────────────────────────────────────

    /// Строка статуса собрана полностью: ровно 7 спанов с точным содержимым.
    #[test]
    fn auth_status_line_builds_all_seven_spans() {
        let line = auth_status_line(
            "Codex",
            true,
            true,
            true,
            "",
            "codex login",
            "C",
            Language::En,
            Theme::Purple,
        );

        // 227:5 `→ Default::default()`: пустая Line без спанов.
        assert_eq!(
            span_texts(&line),
            vec![
                "Codex   ".to_string(),
                "needed      ".to_string(),
                "account ready".to_string(),
                " · ".to_string(),
                "C: codex login".to_string(),
                " · ".to_string(),
                String::new(),
            ]
        );
    }

    /// Не установленный CLI показывает «CLI missing», а не статус авторизации.
    #[test]
    fn auth_status_line_reports_a_missing_cli() {
        let line = auth_status_line(
            "Codex",
            true,
            false,
            false,
            "",
            "codex login",
            "C",
            Language::En,
            Theme::Purple,
        );

        // 232:21 `delete !`: с `if installed` вместо `if !installed` ушли бы в
        // else-ветку → «not logged in».
        assert_eq!(line.spans[2].content.as_ref(), "CLI missing");
    }

    /// Установленный, но не авторизованный CLI подсвечивается жёлтым.
    #[test]
    fn auth_status_line_colours_an_unauthenticated_cli_yellow() {
        let line = auth_status_line(
            "Codex",
            true,
            true,
            false,
            "",
            "codex login",
            "C",
            Language::En,
            Theme::Purple,
        );

        // 239:37 `&& → ||`: installed && authenticated = false → ветка `else if
        // installed` → Yellow. При `||` было бы true → accent_soft (Indexed(183)).
        assert_eq!(line.spans[2].style.fg, Some(Color::Yellow));
        assert_ne!(
            Theme::Purple.accent_soft(),
            Color::Yellow,
            "жёлтый обязан отличаться от accent_soft"
        );
    }
}
