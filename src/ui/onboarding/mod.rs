use super::*;

mod auth;
mod provider;
mod settings;
pub(crate) use auth::*;
pub(crate) use provider::*;
pub(crate) use settings::*;

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

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;
    use ratatui::{backend::TestBackend, buffer::Buffer};

    // ── Хелперы (по образцу src/ui/effort.rs::mod tests) ─────────────────────

    pub(crate) fn onboarding_app() -> App {
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
    pub(crate) fn onboarding(step: OnboardingStep) -> Onboarding {
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

    pub(crate) fn screen(app: &App, width: u16, height: u16) -> Buffer {
        let mut terminal =
            Terminal::new(TestBackend::new(width, height)).expect("оффскрин-терминал");
        terminal
            .draw(|frame| draw_onboarding(frame, frame.area(), app))
            .expect("отрисовка экрана");
        terminal.backend().buffer().clone()
    }

    pub(crate) fn buffer_rows(buffer: &Buffer) -> Vec<String> {
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

    pub(crate) fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    pub(crate) fn span_texts(line: &Line<'_>) -> Vec<String> {
        line.spans
            .iter()
            .map(|span| span.content.to_string())
            .collect()
    }

    pub(crate) fn joined(lines: &[Line<'_>]) -> String {
        lines.iter().map(line_text).collect::<Vec<_>>().join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

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
}
