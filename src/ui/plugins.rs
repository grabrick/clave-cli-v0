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

    // Хедер и подсказка/подтверждение зафиксированы; между ними — скроллируемое тело со
    // списком (реальный каталог claude — сотни доступных, иначе Codex и установленные за краем).
    let header_text = if app.plugins_query.is_empty() {
        "› /plugins".to_string()
    } else {
        format!(
            "› /plugins  {}: {}",
            app.lang.choose("поиск", "search"),
            app.plugins_query
        )
    };
    let header_lines = vec![
        Line::styled(
            header_text,
            Style::default()
                .fg(app.theme.accent())
                .add_modifier(Modifier::BOLD),
        ),
        separator_line(area.width, app.theme),
        Line::from(""),
    ];

    let footer_line = if let Some(pending) = &app.plugins_confirm {
        confirm_line(pending, app.lang)
    } else {
        Line::from(Span::styled(
            app.lang.choose(
                "↑↓ выбор · Enter уст/удал · ^E вкл/выкл · ^U обновить · Tab источники · Esc",
                "↑↓ move · Enter inst/rm · ^E on/off · ^U update · Tab sources · Esc",
            ),
            Style::default().fg(Color::DarkGray),
        ))
    };

    let (body_lines, cursor_row) = plugins_body(app);

    // Раскладка: хедер (3) сверху, подсказка (1) снизу, остальное — тело.
    let header_h = 3u16.min(area.height);
    let footer_h = if area.height > header_h { 1 } else { 0 };
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

    let offset = scroll_offset(cursor_row, body_h as usize, body_lines.len());

    frame.render_widget(Paragraph::new(header_lines), header_area);
    frame.render_widget(Paragraph::new(body_lines).scroll((offset, 0)), body_area);
    if footer_h > 0 {
        frame.render_widget(Paragraph::new(vec![footer_line]), footer_area);
    }
}

/// Строит строки тела панели (секции Claude/Codex со статусами) и позицию строки выделенного
/// плагина в этом списке — по ней прокрутка держит курсор в видимой области.
fn plugins_body(app: &App) -> (Vec<Line<'static>>, usize) {
    // Индекс берём из ОТФИЛЬТРОВАННОГО списка, чтобы выделение совпадало с навигацией/поиском.
    let filtered = app.filtered_plugins();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut cursor_row = 0;
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
            let selected = index == app.plugins_index;
            if selected {
                cursor_row = lines.len();
            }
            lines.push(plugin_line(entry, selected, app.theme, app.lang));
        }
        lines.push(Line::from(""));
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
        app.plugins_index = 0;
        let screen = render(&app);
        // Курсор наверху — виден первый, а не уехавший вниз.
        assert!(
            screen.contains("plugin-00"),
            "верхний плагин виден:\n{screen}"
        );
    }
}
