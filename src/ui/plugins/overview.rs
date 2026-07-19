use super::*;

/// Тело таба «Обзор»: сводка из трёх строк, выбранная подсвечена (Enter уводит в таб).
pub(crate) fn overview_body(app: &App) -> Vec<Line<'static>> {
    let counts = app.plugins_overview();
    let sources_word = if app.marketplaces_loading && counts.sources == 0 {
        app.lang.choose("загрузка…", "loading…").to_string()
    } else {
        counts.sources.to_string()
    };
    let rows = [
        (
            app.lang.choose("Установлено", "Installed"),
            format!(
                "{}   Claude {} · Codex {}",
                counts.installed, counts.claude_installed, counts.codex_installed
            ),
        ),
        (
            app.lang.choose("Доступно", "Available"),
            format!(
                "{}   {} {}",
                counts.available,
                app.lang.choose("в источниках:", "in sources:"),
                sources_word
            ),
        ),
        (
            app.lang.choose("Источники", "Sources"),
            sources_word.clone(),
        ),
    ];

    let mut lines = vec![
        Line::from(Span::styled(
            app.lang.choose("Плагины Clave", "Clave plugins"),
            Style::default()
                .fg(app.theme.accent())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    for (index, (label, value)) in rows.iter().enumerate() {
        let selected = index == app.overview_index;
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
            Span::styled(format!("{label:<12} {value}"), style),
        ]));
    }
    lines
}

pub(crate) fn overview_footer(app: &App) -> Line<'static> {
    Line::from(Span::styled(
        app.lang.choose(
            "↑↓ выбор · Enter — открыть · ↹ таб · 1–4 · Esc",
            "↑↓ move · Enter — open · ↹ tab · 1–4 · Esc",
        ),
        Style::default().fg(Color::DarkGray),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::plugins::testkit::*;

    #[test]
    fn overview_tab_shows_summary_counts() {
        let mut app = app_with(
            vec![
                PluginEntry {
                    provider: Provider::Claude,
                    name: "context7".into(),
                    marketplace: "m".into(),
                    installed: true,
                    enabled: true,
                    version: None,
                },
                PluginEntry {
                    provider: Provider::Codex,
                    name: "documents".into(),
                    marketplace: "m".into(),
                    installed: false,
                    enabled: false,
                    version: None,
                },
            ],
            false,
        );
        app.plugins_tab = PluginsTab::Overview;
        let screen = render(&app);
        assert!(
            screen.contains("Плагины Clave"),
            "заголовок обзора: {screen}"
        );
        assert!(screen.contains("Установлено"), "строка «Установлено»");
        assert!(screen.contains("Доступно"), "строка «Доступно»");
    }
}
