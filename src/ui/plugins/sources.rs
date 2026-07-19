use super::*;

/// Тело таба «Источники»: раздельные секции Claude/Codex со списком marketplace. Выделение
/// сквозное по `marketplaces_index`. Ввод/подтверждение — в подсказке ([`sources_footer`]).
pub(crate) fn marketplace_body(app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for provider in [Provider::Claude, Provider::Codex] {
        lines.push(section_header(provider, app.theme));

        let items: Vec<(usize, &Marketplace)> = app
            .marketplaces
            .iter()
            .enumerate()
            .filter(|(_, m)| m.provider == provider)
            .collect();

        if items.is_empty() {
            let msg = if provider == Provider::Codex && app.marketplaces_loading {
                app.lang.choose("загрузка…", "loading…")
            } else {
                app.lang.choose("нет источников", "no sources")
            };
            lines.push(Line::from(Span::styled(
                format!("  ⎿ {msg}"),
                Style::default().fg(Color::DarkGray),
            )));
        }

        for (index, market) in items {
            lines.push(marketplace_line(
                market,
                index == app.marketplaces_index,
                app.theme,
            ));
        }
        lines.push(Line::from(""));
    }
    lines
}

/// Подсказка таба «Источники»: ввод адреса (2 строки), подтверждение удаления или обычные хинты.
pub(crate) fn sources_footer(app: &App) -> Vec<Line<'static>> {
    if let Some(add) = &app.marketplace_input {
        marketplace_input_lines(add, app.lang)
    } else if let Some(market) = &app.marketplace_confirm {
        vec![marketplace_confirm_line(market, app.lang)]
    } else {
        vec![Line::from(Span::styled(
            app.lang.choose(
                "↑↓ выбор · a добавить · Enter удалить · ↹ таб · Esc",
                "↑↓ move · a add · Enter remove · ↹ tab · Esc",
            ),
            Style::default().fg(Color::DarkGray),
        ))]
    }
}

fn marketplace_line(market: &Marketplace, selected: bool, theme: Theme) -> Line<'static> {
    let text = if market.source.is_empty() {
        truncate_chars(&market.name, 40)
    } else {
        format!(
            "{}  {}",
            truncate_chars(&market.name, 22),
            truncate_chars(&market.source, 36)
        )
    };
    let style = if selected {
        Style::default()
            .fg(Color::White)
            .bg(theme.accent_bg())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.accent_soft())
    };
    Line::from(vec![
        Span::styled(
            if selected { "› " } else { "  " },
            Style::default().fg(theme.accent()),
        ),
        Span::styled(text, style),
    ])
}

/// Две строки ввода адреса: сама строка с целевым провайдером и курсором `▏`, затем подсказка.
fn marketplace_input_lines(add: &MarketplaceAdd, lang: Language) -> Vec<Line<'static>> {
    let provider = match add.provider {
        Provider::Claude => "Claude",
        Provider::Codex => "Codex",
    };
    vec![
        Line::from(Span::styled(
            format!(
                "＋ {} {}: {}▏",
                lang.choose("источник в", "source to"),
                provider,
                add.source,
            ),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            lang.choose(
                "Enter — добавить · Tab — сменить провайдера · Esc — отмена",
                "Enter — add · Tab — switch provider · Esc — cancel",
            ),
            Style::default().fg(Color::DarkGray),
        )),
    ]
}

fn marketplace_confirm_line(market: &Marketplace, lang: Language) -> Line<'static> {
    Line::from(Span::styled(
        format!(
            "⚠ {} {}? Enter — {} · Esc — {}",
            lang.choose("Удалить источник", "Remove source"),
            market.name,
            lang.choose("да", "yes"),
            lang.choose("отмена", "cancel"),
        ),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ))
}

fn section_header(provider: Provider, theme: Theme) -> Line<'static> {
    let label = match provider {
        Provider::Claude => "── Claude ──",
        Provider::Codex => "── Codex ──",
    };
    Line::from(Span::styled(
        label,
        Style::default()
            .fg(theme.accent())
            .add_modifier(Modifier::BOLD),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::plugins::testkit::*;

    fn market(provider: Provider, name: &str, source: &str) -> Marketplace {
        Marketplace {
            provider,
            name: name.to_string(),
            source: source.to_string(),
        }
    }

    #[test]
    fn marketplace_mode_shows_sources_in_both_sections() {
        let mut app = app_with(vec![], false);
        app.plugins_tab = PluginsTab::Sources;
        app.marketplaces = vec![
            market(Provider::Claude, "official", "anthropics/official"),
            market(Provider::Codex, "openai-bundled", "/local/openai"),
        ];
        let screen = render(&app);

        assert!(
            screen.contains("Источники"),
            "таб источников активен: {screen}"
        );
        assert!(
            screen.contains("Claude") && screen.contains("Codex"),
            "обе секции"
        );
        assert!(screen.contains("official"), "claude-источник");
        assert!(screen.contains("anthropics/official"), "адрес источника");
        assert!(screen.contains("openai-bundled"), "codex-источник");
        assert!(screen.contains("a добавить"), "подсказка добавления");
    }

    #[test]
    fn marketplace_add_input_shows_target_provider_and_hint() {
        let mut app = app_with(vec![], false);
        app.plugins_tab = PluginsTab::Sources;
        app.marketplace_input = Some(MarketplaceAdd {
            provider: Provider::Claude,
            source: "anth".to_string(),
        });
        let screen = render(&app);

        assert!(
            screen.contains("источник в Claude"),
            "видна цель добавления: {screen}"
        );
        assert!(screen.contains("anth"), "набранный адрес виден");
        assert!(
            screen.contains("сменить провайдера"),
            "подсказка про Tab: {screen}"
        );
    }

    #[test]
    fn marketplace_confirm_shows_remove_prompt() {
        let mut app = app_with(vec![], false);
        app.plugins_tab = PluginsTab::Sources;
        app.marketplace_confirm = Some(market(Provider::Codex, "openai-bundled", "/local"));
        let screen = render(&app);

        assert!(
            screen.contains("Удалить источник") && screen.contains("openai-bundled"),
            "строка подтверждения удаления: {screen}"
        );
    }

    #[test]
    fn marketplace_empty_codex_shows_loading_while_pending() {
        let mut app = app_with(vec![], false);
        app.plugins_tab = PluginsTab::Sources;
        app.marketplaces_loading = true;
        app.marketplaces = vec![market(Provider::Claude, "official", "anthropics/official")];
        let screen = render(&app);

        assert!(
            screen.contains("загрузка"),
            "секция codex грузится: {screen}"
        );
    }
}
