use super::*;

/// Панель `/plugins`: таб-бар (Обзор/Установленные/Каталог/Источники) под шапкой, тело — по
/// активному табу. Вход на «Обзор» (сводка), чтобы не вываливать сотни доступных плагинов сразу;
/// списки — в «Установленных»/«Каталоге» (со скроллом и поиском), источники — в своём табе.
pub(crate) fn draw_plugins_screen(frame: &mut Frame<'_>, area: Rect, app: &App) {
    frame.render_widget(Clear, area);

    let header = plugins_header_lines(app, area.width);
    // Тело и подсказка зависят от таба; `cursor_row` нужен только списку (там скролл).
    let (body, cursor_row, footer) = match app.plugins_tab {
        PluginsTab::Overview => (overview_body(app), 0, vec![overview_footer(app)]),
        PluginsTab::Installed | PluginsTab::Catalog => {
            let (body, cursor_row) = plugins_body(app);
            (body, cursor_row, vec![plugin_list_footer(app)])
        }
        PluginsTab::Sources => (marketplace_body(app), 0, sources_footer(app)),
    };
    render_paneled(frame, area, header, body, cursor_row, footer);
}

/// Хедер панели: заголовок (+ поиск в Каталоге), таб-бар, разделитель, пустая строка.
fn plugins_header_lines(app: &App, width: u16) -> Vec<Line<'static>> {
    let title = if app.plugins_tab == PluginsTab::Catalog && !app.plugins_query.is_empty() {
        format!(
            "› /plugins  {}: {}",
            app.lang.choose("поиск", "search"),
            app.plugins_query
        )
    } else {
        "› /plugins".to_string()
    };
    let mut lines = vec![
        Line::styled(
            title,
            Style::default()
                .fg(app.theme.accent())
                .add_modifier(Modifier::BOLD),
        ),
        tab_bar_line(app),
    ];
    // В списковых табах — строка выбора провайдера (переключается ←/→).
    if matches!(app.plugins_tab, PluginsTab::Installed | PluginsTab::Catalog) {
        lines.push(provider_selector_line(app));
    }
    lines.push(separator_line(width, app.theme));
    lines.push(Line::from(""));
    lines
}

/// Строка выбора провайдера в списковых табах: `Claude`/`Codex`, активный подсвечен.
fn provider_selector_line(app: &App) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!("{}: ", app.lang.choose("Провайдер", "Provider")),
        Style::default().fg(Color::DarkGray),
    )];
    for provider in [Provider::Claude, Provider::Codex] {
        let label = match provider {
            Provider::Claude => "Claude",
            Provider::Codex => "Codex",
        };
        let style = if provider == app.plugins_provider {
            Style::default()
                .fg(Color::White)
                .bg(app.theme.accent_bg())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.accent_soft())
        };
        spans.push(Span::styled(format!(" {label} "), style));
        spans.push(Span::raw(" "));
    }
    Line::from(spans)
}

/// Таб-бар: метки всех табов, активный подсвечен.
fn tab_bar_line(app: &App) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, tab) in PluginsTab::ALL.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        let style = if *tab == app.plugins_tab {
            Style::default()
                .fg(Color::White)
                .bg(app.theme.accent_bg())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.accent_soft())
        };
        spans.push(Span::styled(format!(" {} ", tab.label(app.lang)), style));
    }
    Line::from(spans)
}

/// Общая раскладка: фиксированные хедер сверху и подсказка снизу, скроллируемое тело между.
fn render_paneled(
    frame: &mut Frame<'_>,
    area: Rect,
    header: Vec<Line<'static>>,
    body: Vec<Line<'static>>,
    cursor_row: usize,
    footer: Vec<Line<'static>>,
) {
    let header_h = (header.len() as u16).min(area.height);
    let footer_h = (footer.len() as u16).min(area.height.saturating_sub(header_h));
    let body_h = area.height.saturating_sub(header_h + footer_h);
    let header_area = Rect {
        height: header_h,
        ..area
    };
    let body_area = Rect {
        y: area.y + header_h,
        height: body_h,
        ..area
    };
    let footer_area = Rect {
        y: area.y + header_h + body_h,
        height: footer_h,
        ..area
    };
    let offset = scroll_offset(cursor_row, body_h as usize, body.len());
    frame.render_widget(Paragraph::new(header), header_area);
    frame.render_widget(Paragraph::new(body).scroll((offset, 0)), body_area);
    if footer_h > 0 {
        frame.render_widget(Paragraph::new(footer), footer_area);
    }
}

/// Тело таба «Обзор»: сводка из трёх строк, выбранная подсвечена (Enter уводит в таб).
fn overview_body(app: &App) -> Vec<Line<'static>> {
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

fn overview_footer(app: &App) -> Line<'static> {
    Line::from(Span::styled(
        app.lang.choose(
            "↑↓ выбор · Enter — открыть · ↹ таб · 1–4 · Esc",
            "↑↓ move · Enter — open · ↹ tab · 1–4 · Esc",
        ),
        Style::default().fg(Color::DarkGray),
    ))
}

fn plugin_list_footer(app: &App) -> Line<'static> {
    if let Some(pending) = &app.plugins_confirm {
        return confirm_line(pending, app.lang);
    }
    let hint = match app.plugins_tab {
        PluginsTab::Installed => app.lang.choose(
            "↑↓ · ←→ провайдер · Enter удалить · ^E вкл/выкл · ^U обновить · ↹ таб · Esc",
            "↑↓ · ←→ provider · Enter remove · ^E on/off · ^U update · ↹ tab · Esc",
        ),
        _ => app.lang.choose(
            "↑↓ · ←→ провайдер · Enter установить · поиск · ↹ таб · Esc",
            "↑↓ · ←→ provider · Enter install · search · ↹ tab · Esc",
        ),
    };
    Line::from(Span::styled(hint, Style::default().fg(Color::DarkGray)))
}

/// Строит строки тела спискового таба (плагины ОДНОГО провайдера — он выбран в шапке) и позицию
/// строки выделенного плагина — по ней прокрутка держит курсор в видимой области.
fn plugins_body(app: &App) -> (Vec<Line<'static>>, usize) {
    // Список уже отфильтрован по табу+провайдеру+поиску, порядок совпадает с навигацией.
    let filtered = app.filtered_plugins();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut cursor_row = 0;

    if filtered.is_empty() {
        let msg = if app.plugins_provider == Provider::Codex && app.plugins_loading {
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

    for (index, entry) in filtered.iter().enumerate() {
        let selected = index == app.plugins_index;
        if selected {
            cursor_row = lines.len();
        }
        lines.push(plugin_line(entry, selected, app.theme, app.lang));
    }
    (lines, cursor_row)
}

/// Смещение прокрутки тела: пока выделенная строка (`cursor_row`) помещается в окно высотой
/// `viewport`, смещения нет; ниже — окно едет за курсором, но не дальше конца списка (`total`).
fn scroll_offset(cursor_row: usize, viewport: usize, total: usize) -> u16 {
    if viewport == 0 || total <= viewport {
        return 0;
    }
    let max_offset = total - viewport;
    cursor_row.saturating_sub(viewport - 1).min(max_offset) as u16
}

/// Тело таба «Источники»: раздельные секции Claude/Codex со списком marketplace. Выделение
/// сквозное по `marketplaces_index`. Ввод/подтверждение — в подсказке ([`sources_footer`]).
fn marketplace_body(app: &App) -> Vec<Line<'static>> {
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
fn sources_footer(app: &App) -> Vec<Line<'static>> {
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
    fn installed_and_catalog_tabs_show_their_plugins_with_statuses() {
        let mut app = app_with(
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

        // Таб «Установленные» — только установленный, маркер ● и версия; доступного тут нет.
        app.plugins_tab = PluginsTab::Installed;
        let installed = render(&app);
        assert!(installed.contains("context7"), "установленный: {installed}");
        assert!(installed.contains("●"), "маркер установленного");
        assert!(installed.contains("v1.2"), "версия установленного");
        assert!(
            !installed.contains("documents"),
            "доступного нет в Установленных"
        );

        // Таб «Каталог» + провайдер Codex — доступный documents с маркером ○.
        app.plugins_tab = PluginsTab::Catalog;
        app.plugins_provider = Provider::Codex;
        let catalog = render(&app);
        assert!(catalog.contains("documents"), "доступный: {catalog}");
        assert!(catalog.contains("○"), "маркер доступного");
        assert!(
            !catalog.contains("context7"),
            "установленного нет в Каталоге"
        );
    }

    #[test]
    fn empty_codex_section_shows_loading_while_pending() {
        let mut app = app_with(
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
        app.plugins_tab = PluginsTab::Installed;
        app.plugins_provider = Provider::Codex; // codex ещё грузится → секция пуста
        let screen = render(&app);
        assert!(screen.contains("загрузка"), "codex ещё грузится: {screen}");
    }

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

    #[test]
    fn tab_bar_lists_all_tabs() {
        let app = app_with(vec![], false); // вход — Обзор
        let screen = render(&app);
        for label in ["Обзор", "Установленные", "Каталог", "Источники"]
        {
            assert!(screen.contains(label), "таб «{label}» в баре: {screen}");
        }
    }

    #[test]
    fn catalog_provider_toggle_reaches_codex() {
        let mut app = app_with(
            vec![
                PluginEntry {
                    provider: Provider::Claude,
                    name: "claude-avail".into(),
                    marketplace: "m".into(),
                    installed: false,
                    enabled: false,
                    version: None,
                },
                PluginEntry {
                    provider: Provider::Codex,
                    name: "codex-avail".into(),
                    marketplace: "m".into(),
                    installed: false,
                    enabled: false,
                    version: None,
                },
            ],
            false,
        );
        app.plugins_tab = PluginsTab::Catalog;

        // Провайдер Claude: строка выбора видна, claude-плагин показан, codex скрыт.
        let claude_view = render(&app);
        assert!(
            claude_view.contains("Провайдер"),
            "строка выбора: {claude_view}"
        );
        assert!(claude_view.contains("claude-avail"), "claude-плагин виден");
        assert!(
            !claude_view.contains("codex-avail"),
            "codex скрыт под Claude, а не погребён снизу"
        );

        // ←/→ на Codex — теперь codex-плагин достижим сразу.
        app.plugins_provider = Provider::Codex;
        let codex_view = render(&app);
        assert!(
            codex_view.contains("codex-avail"),
            "codex достижим сменой провайдера: {codex_view}"
        );
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

    #[test]
    fn selected_far_plugin_scrolls_into_view() {
        // Список длиннее экрана: выделенный на дне плагин обязан прокрутиться в видимую область.
        // Реальный каталог claude — сотни доступных; без вьюпорта видны только первые буквы.
        let plugins: Vec<PluginEntry> = (0..40)
            .map(|i| PluginEntry {
                provider: Provider::Claude,
                name: format!("plugin-{i:02}"),
                marketplace: "m".into(),
                installed: false,
                enabled: false,
                version: None,
            })
            .collect();
        let mut app = app_with(plugins, false);
        app.plugins_tab = PluginsTab::Catalog; // доступные — в Каталоге
        app.plugins_index = 39;
        let screen = render(&app);
        assert!(
            screen.contains("plugin-39"),
            "дальний выделенный плагин должен прокрутиться в вид:\n{screen}"
        );
    }

    #[test]
    fn scroll_offset_keeps_cursor_in_view() {
        // Список помещается целиком — прокрутки нет.
        assert_eq!(scroll_offset(0, 10, 5), 0);
        assert_eq!(scroll_offset(4, 10, 5), 0);
        // Курсор в пределах первого окна — смещения нет.
        assert_eq!(scroll_offset(9, 10, 100), 0);
        // Курсор ниже окна — оно едет ровно настолько, чтобы курсор был виден снизу.
        assert_eq!(scroll_offset(10, 10, 100), 1);
        assert_eq!(scroll_offset(20, 10, 100), 11);
        // У самого дна — окно упирается в конец, не дальше.
        assert_eq!(scroll_offset(99, 10, 100), 90);
        // Вырожденное окно — защита от переполнения.
        assert_eq!(scroll_offset(5, 0, 100), 0);
    }

    #[test]
    fn top_plugin_visible_and_no_scroll_when_fits() {
        let plugins: Vec<PluginEntry> = (0..40)
            .map(|i| PluginEntry {
                provider: Provider::Claude,
                name: format!("plugin-{i:02}"),
                marketplace: "m".into(),
                installed: false,
                enabled: false,
                version: None,
            })
            .collect();
        let mut app = app_with(plugins, false);
        app.plugins_tab = PluginsTab::Catalog;
        app.plugins_index = 0;
        let screen = render(&app);
        // Курсор наверху — виден первый, а не уехавший вниз.
        assert!(
            screen.contains("plugin-00"),
            "верхний плагин виден:\n{screen}"
        );
    }
}
