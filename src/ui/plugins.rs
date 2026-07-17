use super::*;

/// Панель `/plugins`: плагины обоих провайдеров раздельными секциями (Claude, затем Codex).
/// Фаза 1 — только просмотр: статусы `●`установлен/`○`доступен, вкл/выкл, версия. Выделение
/// сквозное по `plugins_index` (навигация идёт по объединённому списку `app.plugins`).
pub(crate) fn draw_plugins_screen(frame: &mut Frame<'_>, area: Rect, app: &App) {
    frame.render_widget(Clear, area);

    // Режим источников — свой рендер (список marketplace обоих провайдеров + ввод/подтверждение).
    if app.plugins_marketplace_mode {
        draw_marketplace_screen(frame, area, app);
        return;
    }

    let header = if app.plugins_query.is_empty() {
        "› /plugins".to_string()
    } else {
        format!(
            "› /plugins  {}: {}",
            app.lang.choose("поиск", "search"),
            app.plugins_query
        )
    };
    let mut lines = vec![
        Line::styled(
            header,
            Style::default()
                .fg(app.theme.accent())
                .add_modifier(Modifier::BOLD),
        ),
        separator_line(area.width, app.theme),
        Line::from(""),
    ];

    // Индекс берём из ОТФИЛЬТРОВАННОГО списка, чтобы выделение совпадало с навигацией/поиском.
    let filtered = app.filtered_plugins();
    for provider in [Provider::Claude, Provider::Codex] {
        lines.push(section_header(provider, app.theme));

        let items: Vec<(usize, &PluginEntry)> = filtered
            .iter()
            .enumerate()
            .filter(|(_, p)| p.provider == provider)
            .map(|(index, entry)| (index, *entry))
            .collect();

        if items.is_empty() {
            let msg = if provider == Provider::Codex && app.plugins_loading {
                app.lang.choose("загрузка…", "loading…")
            } else if !app.plugins_query.is_empty() {
                app.lang.choose("ничего не найдено", "nothing found")
            } else {
                app.lang.choose("нет плагинов", "no plugins")
            };
            lines.push(Line::from(Span::styled(
                format!("  ⎿ {msg}"),
                Style::default().fg(Color::DarkGray),
            )));
        }

        for (index, entry) in items {
            lines.push(plugin_line(
                entry,
                index == app.plugins_index,
                app.theme,
                app.lang,
            ));
        }
        lines.push(Line::from(""));
    }

    // Строка подтверждения перекрывает подсказки, пока действие ждёт да/отмену.
    if let Some(pending) = &app.plugins_confirm {
        lines.push(confirm_line(pending, app.lang));
    } else {
        lines.push(Line::from(Span::styled(
            app.lang.choose(
                "↑↓ выбор · Enter уст/удал · ^E вкл/выкл · ^U обновить · Tab источники · Esc",
                "↑↓ move · Enter inst/rm · ^E on/off · ^U update · Tab sources · Esc",
            ),
            Style::default().fg(Color::DarkGray),
        )));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

/// Рендер режима marketplace-источников: раздельные секции Claude/Codex, снизу — строка ввода
/// нового адреса, строка подтверждения удаления или подсказки. Выделение сквозное по
/// `marketplaces_index` (курсор идёт по объединённому списку `app.marketplaces`).
fn draw_marketplace_screen(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines = vec![
        Line::styled(
            app.lang
                .choose("› /plugins · источники", "› /plugins · sources"),
            Style::default()
                .fg(app.theme.accent())
                .add_modifier(Modifier::BOLD),
        ),
        separator_line(area.width, app.theme),
        Line::from(""),
    ];

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

    // Ввод адреса / подтверждение удаления перекрывают подсказки.
    if let Some(add) = &app.marketplace_input {
        lines.extend(marketplace_input_lines(add, app.lang));
    } else if let Some(market) = &app.marketplace_confirm {
        lines.push(marketplace_confirm_line(market, app.lang));
    } else {
        lines.push(Line::from(Span::styled(
            app.lang.choose(
                "↑↓ выбор · a добавить · Enter удалить · Tab плагины · Esc",
                "↑↓ move · a add · Enter remove · Tab plugins · Esc",
            ),
            Style::default().fg(Color::DarkGray),
        )));
    }

    frame.render_widget(Paragraph::new(lines), area);
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

fn confirm_line(pending: &PendingPluginAction, lang: Language) -> Line<'static> {
    let verb = match pending.action {
        PluginAction::Install => lang.choose("Установить", "Install"),
        PluginAction::Uninstall => lang.choose("Удалить", "Uninstall"),
        _ => lang.choose("Применить", "Apply"),
    };
    Line::from(Span::styled(
        format!(
            "⚠ {verb} {}? Enter — {} · Esc — {}",
            pending.entry.qualified_name(),
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

fn plugin_line(entry: &PluginEntry, selected: bool, theme: Theme, lang: Language) -> Line<'static> {
    let marker = if entry.installed { "●" } else { "○" };
    let toggle = if !entry.installed {
        String::new()
    } else if entry.enabled {
        format!(" {}", lang.choose("✓вкл", "✓on"))
    } else {
        format!(" {}", lang.choose("✕выкл", "✕off"))
    };
    let version = entry
        .version
        .as_deref()
        .map(|v| format!(" v{v}"))
        .unwrap_or_default();
    let status = if entry.installed {
        lang.choose("уст.", "inst.")
    } else {
        lang.choose("дост.", "avail.")
    };
    let text = format!(
        "{marker} {}{toggle}{version}  {status}",
        truncate_chars(&entry.name, 32)
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    fn render(app: &App) -> String {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 24)).unwrap();
        terminal
            .draw(|f| draw_plugins_screen(f, f.area(), app))
            .unwrap();
        buffer_rows(terminal.backend().buffer()).join("\n")
    }

    fn buffer_rows(buffer: &ratatui::buffer::Buffer) -> Vec<String> {
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

    fn app_with(plugins: Vec<PluginEntry>, loading: bool) -> App {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "clave-uiplug-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::create_dir_all(&dir);
        let mut app = App::from_config(
            AppConfig {
                onboarding_done: true,
                ..AppConfig::default()
            },
            dir.join("config.json"),
            dir.join("history"),
            dir.clone(),
        );
        app.onboarding = None;
        app.plugins = plugins;
        app.plugins_loading = loading;
        app.lang = Language::Ru;
        app
    }

    #[test]
    fn shows_both_sections_with_statuses() {
        let app = app_with(
            vec![
                PluginEntry {
                    provider: Provider::Claude,
                    name: "context7".into(),
                    marketplace: "official".into(),
                    installed: true,
                    enabled: true,
                    version: Some("1.2".into()),
                },
                PluginEntry {
                    provider: Provider::Codex,
                    name: "documents".into(),
                    marketplace: "openai".into(),
                    installed: false,
                    enabled: false,
                    version: None,
                },
            ],
            false,
        );
        let screen = render(&app);
        assert!(screen.contains("Claude"), "секция Claude: {screen}");
        assert!(screen.contains("Codex"), "секция Codex: {screen}");
        assert!(screen.contains("context7"), "плагин claude");
        assert!(screen.contains("documents"), "плагин codex");
        assert!(
            screen.contains("●") && screen.contains("○"),
            "маркеры уст/дост"
        );
        assert!(screen.contains("v1.2"), "версия установленного");
    }

    #[test]
    fn empty_codex_section_shows_loading_while_pending() {
        let app = app_with(
            vec![PluginEntry {
                provider: Provider::Claude,
                name: "context7".into(),
                marketplace: "official".into(),
                installed: true,
                enabled: true,
                version: None,
            }],
            true,
        );
        let screen = render(&app);
        assert!(screen.contains("загрузка"), "codex ещё грузится: {screen}");
    }

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
        app.plugins_marketplace_mode = true;
        app.marketplaces = vec![
            market(Provider::Claude, "official", "anthropics/official"),
            market(Provider::Codex, "openai-bundled", "/local/openai"),
        ];
        let screen = render(&app);

        assert!(screen.contains("источники"), "заголовок режима: {screen}");
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
        app.plugins_marketplace_mode = true;
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
        app.plugins_marketplace_mode = true;
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
        app.plugins_marketplace_mode = true;
        app.marketplaces_loading = true;
        app.marketplaces = vec![market(Provider::Claude, "official", "anthropics/official")];
        let screen = render(&app);

        assert!(
            screen.contains("загрузка"),
            "секция codex грузится: {screen}"
        );
    }
}
