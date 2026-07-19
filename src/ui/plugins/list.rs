use super::*;

/// Строка выбора провайдера в списковых табах: `Claude`/`Codex`, активный подсвечен.
pub(crate) fn provider_selector_line(app: &App) -> Line<'static> {
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

/// Низ списковых табов: область деталей выбранного плагина (описание/автор) + строка подсказки.
/// Пока действие ждёт подтверждения — вместо всего этого строка подтверждения.
pub(crate) fn plugin_list_footer(app: &App, width: u16) -> Vec<Line<'static>> {
    if let Some(pending) = &app.plugins_confirm {
        return vec![confirm_line(pending, app.lang)];
    }
    let mut lines = plugin_detail_lines(app, width);
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
    lines.push(Line::from(Span::styled(
        hint,
        Style::default().fg(Color::DarkGray),
    )));
    lines
}

/// Область деталей выбранного плагина: разделитель, имя, описание (перенос до 2 строк, если есть),
/// автор и ИСТОЧНИК (маркетплейс). Имя и источник показываем всегда — видно, откуда плагин; для
/// codex/без описания — только они. Пусто, если в табе нет выбранного плагина.
fn plugin_detail_lines(app: &App, width: u16) -> Vec<Line<'static>> {
    let Some(entry) = app.filtered_plugins().get(app.plugins_index).copied() else {
        return Vec::new();
    };
    let mut lines = vec![
        separator_line(width, app.theme),
        Line::from(Span::styled(
            format!("● {}", entry.name),
            Style::default()
                .fg(app.theme.accent())
                .add_modifier(Modifier::BOLD),
        )),
    ];
    if let Some(detail) = app.plugin_details.get(&entry.qualified_name()) {
        for line in wrap_text(&detail.description, width.saturating_sub(2) as usize, 2) {
            lines.push(Line::from(Span::styled(
                format!("  {line}"),
                Style::default().fg(app.theme.accent_soft()),
            )));
        }
        if let Some(author) = &detail.author {
            lines.push(Line::from(Span::styled(
                format!("  ↳ {author}"),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }
    if !entry.marketplace.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  ⌂ {}", entry.marketplace),
            Style::default().fg(Color::DarkGray),
        )));
    }
    lines
}

/// Жадный перенос текста по словам в строки шириной `width`, не больше `max_lines`. Если текст
/// длиннее — последняя строка усечётся с «…». Для описания плагина в области деталей.
fn wrap_text(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    if width == 0 || max_lines == 0 {
        return Vec::new();
    }
    let mut all: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let fits = current.chars().count() + 1 + word.chars().count() <= width;
        if current.is_empty() {
            current.push_str(word);
        } else if fits {
            current.push(' ');
            current.push_str(word);
        } else {
            all.push(std::mem::take(&mut current));
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        all.push(current);
    }
    if all.len() > max_lines {
        all.truncate(max_lines);
        if let Some(last) = all.last_mut() {
            let trimmed: String = last.chars().take(width.saturating_sub(1)).collect();
            *last = format!("{trimmed}…");
        }
    }
    all
}

/// Строит строки тела спискового таба (плагины ОДНОГО провайдера — он выбран в шапке) и позицию
/// строки выделенного плагина — по ней прокрутка держит курсор в видимой области.
pub(crate) fn plugins_body(app: &App, width: u16) -> (Vec<Line<'static>>, usize) {
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
        let has_update = app.plugin_updates.contains(&entry.qualified_name());
        lines.push(plugin_line(entry, selected, app.theme, width, has_update));
    }
    (lines, cursor_row)
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

/// Строка плагина. У установленного точка кодирует СОСТОЯНИЕ: `●` включён (акцент), `○` выключен
/// (серый). У доступного (Каталог) состояния нет — точки нет, только имя. Версия — приглушённой у
/// правого края отдельной колонкой. Без слов «вкл/выкл» и «уст.» — их несут точка и сам таб.
fn plugin_line(
    entry: &PluginEntry,
    selected: bool,
    theme: Theme,
    width: u16,
    has_update: bool,
) -> Line<'static> {
    let prefix = if selected { "› " } else { "  " };
    let (marker, marker_style) = if !entry.installed {
        ("", Style::default())
    } else if entry.enabled {
        ("● ", Style::default().fg(theme.accent()))
    } else {
        ("○ ", Style::default().fg(Color::DarkGray))
    };
    let name = truncate_chars(&entry.name, 44);
    let version = entry
        .version
        .as_deref()
        .map(|v| format!("v{v}"))
        .unwrap_or_default();

    let name_style = if selected {
        Style::default()
            .fg(Color::White)
            .bg(theme.accent_bg())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.accent_soft())
    };

    let mut spans = vec![
        Span::styled(prefix, Style::default().fg(theme.accent())),
        Span::styled(marker, marker_style),
        Span::styled(name.clone(), name_style),
    ];
    // Версия отдельной колонкой у правого края. Есть обновление → бейдж «↑ vX» жёлтым, иначе
    // версия приглушённая.
    if !version.is_empty() {
        let (vtext, vstyle) = if has_update {
            (
                format!("↑ {version}"),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            (version.clone(), Style::default().fg(Color::DarkGray))
        };
        let used = prefix.chars().count()
            + marker.chars().count()
            + name.chars().count()
            + vtext.chars().count();
        let pad = (width as usize).saturating_sub(used + 1).max(1);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(vtext, vstyle));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::plugins::testkit::*;

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

        // Таб «Установленные» — только установленный, включённый → точка ● и версия.
        app.plugins_tab = PluginsTab::Installed;
        let installed = render(&app);
        assert!(installed.contains("context7"), "установленный: {installed}");
        assert!(installed.contains("●"), "точка включённого");
        assert!(installed.contains("v1.2"), "версия установленного");
        assert!(
            !installed.contains("documents"),
            "доступного нет в Установленных"
        );

        // Таб «Каталог» + провайдер Codex — доступный documents (у доступных точки состояния нет).
        app.plugins_tab = PluginsTab::Catalog;
        app.plugins_provider = Provider::Codex;
        let catalog = render(&app);
        assert!(catalog.contains("documents"), "доступный: {catalog}");
        assert!(
            !catalog.contains("context7"),
            "установленного нет в Каталоге"
        );
    }

    #[test]
    fn installed_dot_encodes_enabled_state_without_status_words() {
        let mut app = app_with(
            vec![
                PluginEntry {
                    provider: Provider::Claude,
                    name: "on-plugin".into(),
                    marketplace: "m".into(),
                    installed: true,
                    enabled: true,
                    version: Some("1.0".into()),
                },
                PluginEntry {
                    provider: Provider::Claude,
                    name: "off-plugin".into(),
                    marketplace: "m".into(),
                    installed: true,
                    enabled: false,
                    version: None,
                },
            ],
            false,
        );
        app.plugins_tab = PluginsTab::Installed;
        let screen = render(&app);

        // Точка кодирует состояние: включённый ●, выключенный ○.
        assert!(screen.contains("●"), "включённый — полная точка: {screen}");
        assert!(screen.contains("○"), "выключенный — пустая точка: {screen}");
        // В самой строке плагина нет слов-статусов «вкл»/«уст.» (их несут точка и таб).
        let on_line = screen
            .lines()
            .find(|line| line.contains("on-plugin"))
            .unwrap_or_default();
        assert!(
            !on_line.contains("вкл") && !on_line.contains("уст"),
            "строка плагина без слов-статусов: {on_line:?}"
        );
        assert!(on_line.contains("v1.0"), "версия в строке: {on_line:?}");
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

    #[test]
    fn update_available_shows_arrow_badge_in_the_row() {
        let mut app = app_with(
            vec![PluginEntry {
                provider: Provider::Claude,
                name: "outdated".into(),
                marketplace: "m".into(),
                installed: true,
                enabled: true,
                version: Some("2.3.0".into()),
            }],
            false,
        );
        app.plugins_tab = PluginsTab::Installed;

        // Есть обновление → в СТРОКЕ плагина стрелка ↑ и версия (подсказка «↑↓» не в счёт).
        app.plugin_updates.insert("outdated@m".to_string());
        let with_update = render(&app);
        let up_line = with_update
            .lines()
            .find(|line| line.contains("outdated"))
            .unwrap_or_default();
        assert!(
            up_line.contains("↑"),
            "бейдж обновления в строке: {up_line:?}"
        );
        assert!(up_line.contains("v2.3.0"), "версия в строке: {up_line:?}");

        // Нет обновления → в строке стрелки нет.
        app.plugin_updates.clear();
        let plain = render(&app);
        let plain_line = plain
            .lines()
            .find(|line| line.contains("outdated"))
            .unwrap_or_default();
        assert!(
            !plain_line.contains("↑"),
            "без апдейта стрелки в строке нет: {plain_line:?}"
        );
    }

    #[test]
    fn detail_area_shows_description_and_author_of_selected() {
        let mut app = app_with(
            vec![PluginEntry {
                provider: Provider::Claude,
                name: "context7".into(),
                marketplace: "official".into(),
                installed: false,
                enabled: false,
                version: None,
            }],
            false,
        );
        app.plugins_tab = PluginsTab::Catalog;
        app.plugins_index = 0;
        app.plugin_details.insert(
            "context7@official".to_string(),
            PluginDetail {
                description: "Up-to-date library docs for any prompt".into(),
                author: Some("upstash".into()),
            },
        );

        let screen = render(&app);
        assert!(
            screen.contains("Up-to-date library docs"),
            "описание выбранного плагина в деталях: {screen}"
        );
        assert!(screen.contains("upstash"), "автор в деталях: {screen}");
    }

    #[test]
    fn wrap_text_wraps_and_truncates_with_ellipsis() {
        // Длинный текст → ровно max_lines, последняя усечена «…».
        let lines = wrap_text("one two three four five six seven", 9, 2);
        assert_eq!(lines.len(), 2, "не больше max_lines: {lines:?}");
        assert!(lines[0].chars().count() <= 9, "строка в пределах ширины");
        assert!(
            lines.last().unwrap().ends_with('…'),
            "усечение многоточием: {lines:?}"
        );
        // Короткий — одна строка без усечения.
        assert_eq!(wrap_text("hi there", 20, 2), vec!["hi there".to_string()]);
        // Вырожденная ширина — пусто, без паники.
        assert!(wrap_text("x", 0, 2).is_empty());
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
