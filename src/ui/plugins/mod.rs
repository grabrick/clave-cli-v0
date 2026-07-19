use super::*;

mod list;
mod overview;
mod sources;
pub(crate) use list::*;
pub(crate) use overview::*;
pub(crate) use sources::*;

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
            let (body, cursor_row) = plugins_body(app, area.width);
            (body, cursor_row, plugin_list_footer(app, area.width))
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

/// Смещение прокрутки тела: пока выделенная строка (`cursor_row`) помещается в окно высотой
/// `viewport`, смещения нет; ниже — окно едет за курсором, но не дальше конца списка (`total`).
fn scroll_offset(cursor_row: usize, viewport: usize, total: usize) -> u16 {
    if viewport == 0 || total <= viewport {
        return 0;
    }
    let max_offset = total - viewport;
    cursor_row.saturating_sub(viewport - 1).min(max_offset) as u16
}

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;

    pub(crate) fn render(app: &App) -> String {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 24)).unwrap();
        terminal
            .draw(|f| draw_plugins_screen(f, f.area(), app))
            .unwrap();
        buffer_rows(terminal.backend().buffer()).join("\n")
    }

    pub(crate) fn buffer_rows(buffer: &ratatui::buffer::Buffer) -> Vec<String> {
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

    pub(crate) fn app_with(plugins: Vec<PluginEntry>, loading: bool) -> App {
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
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

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
}
