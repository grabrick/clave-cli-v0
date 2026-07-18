use super::*;

/// Клавиши панели плагинов: навигация, поиск (прямой ввод), действия и подтверждение.
pub(crate) fn handle_plugins_key(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    // Ctrl+C — выход из приложения из любого таба/суб-состояния.
    if ctrl && matches!(key.code, KeyCode::Char('c')) {
        app.handle_ctrl_c();
        return;
    }

    // Суб-состояния таба «Источники» перехватывают ввод целиком.
    if app.marketplace_input.is_some() {
        match key.code {
            KeyCode::Enter => app.marketplace_submit_add(),
            KeyCode::Esc => app.marketplace_cancel_input(),
            KeyCode::Tab => app.marketplace_toggle_add_provider(),
            KeyCode::Backspace => app.marketplace_input_backspace(),
            KeyCode::Char(c) if !ctrl => app.marketplace_input_push(c),
            _ => {}
        }
        return;
    }
    if app.marketplace_confirm.is_some() {
        match key.code {
            KeyCode::Enter => app.confirm_marketplace_remove(),
            KeyCode::Esc => app.cancel_marketplace_remove(),
            _ => {}
        }
        return;
    }
    // Подтверждение действия над плагином (табы «Установленные»/«Каталог»).
    if app.plugins_confirm.is_some() {
        match key.code {
            KeyCode::Enter => app.confirm_plugin_action(),
            KeyCode::Esc => app.cancel_plugin_action(),
            _ => {}
        }
        return;
    }

    // Переключение табов — общее. В Каталоге цифры уходят в поиск, поэтому там прыжок только Tab.
    match key.code {
        KeyCode::Tab => return app.plugins_tab_next(),
        KeyCode::BackTab => return app.plugins_tab_prev(),
        _ => {}
    }

    match app.plugins_tab {
        PluginsTab::Overview => handle_overview_key(app, key),
        PluginsTab::Installed | PluginsTab::Catalog => handle_plugin_list_key(app, key, ctrl),
        PluginsTab::Sources => handle_sources_key(app, key),
    }
}

/// Таб «Обзор»: `↑↓` по строкам сводки, `Enter`/цифра — прыжок в таб, `Esc` — закрыть.
fn handle_overview_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.overview_index = app.overview_index.saturating_sub(1),
        KeyCode::Down => {
            let last = OVERVIEW_ROWS.len().saturating_sub(1);
            app.overview_index = (app.overview_index + 1).min(last);
        }
        KeyCode::Enter => app.overview_enter(),
        KeyCode::Char(c) => {
            if let Some(tab) = tab_for_digit(c) {
                app.set_plugins_tab(tab);
            }
        }
        KeyCode::Esc => app.overlay = Overlay::None,
        _ => {}
    }
}

/// Табы «Установленные»/«Каталог»: список плагинов. Действия на Ctrl-клавишах; в Каталоге буквы
/// уходят в поиск, в Установленных (поиска нет) цифры листают табы.
fn handle_plugin_list_key(app: &mut App, key: KeyEvent, ctrl: bool) {
    match key.code {
        KeyCode::Up => app.plugins_index = app.plugins_index.saturating_sub(1),
        KeyCode::Down => {
            let last = app.filtered_plugins().len().saturating_sub(1);
            app.plugins_index = (app.plugins_index + 1).min(last);
        }
        // ←/→ — переключить провайдера (Claude ⇄ Codex); иначе codex тонет под каталогом claude.
        KeyCode::Left | KeyCode::Right => app.toggle_plugins_provider(),
        KeyCode::Enter => app.plugin_enter(),
        KeyCode::Char('e') if ctrl => app.plugin_toggle(),
        KeyCode::Char('u') if ctrl => app.plugin_update(),
        KeyCode::Char(c) => {
            if app.plugins_tab == PluginsTab::Catalog {
                app.plugins_query.push(c);
                app.plugins_index = 0;
            } else if let Some(tab) = tab_for_digit(c) {
                app.set_plugins_tab(tab);
            }
        }
        KeyCode::Backspace if app.plugins_tab == PluginsTab::Catalog => {
            app.plugins_query.pop();
            app.plugins_index = 0;
        }
        KeyCode::Esc => {
            // В Каталоге Esc сначала снимает поиск, затем закрывает панель.
            if app.plugins_tab == PluginsTab::Catalog && !app.plugins_query.is_empty() {
                app.plugins_query.clear();
                app.plugins_index = 0;
            } else {
                app.overlay = Overlay::None;
            }
        }
        _ => {}
    }
}

/// Таб «Источники»: `a` — добавить, `Enter` — удалить (через подтверждение), цифра — прыжок в таб.
fn handle_sources_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.marketplaces_index = app.marketplaces_index.saturating_sub(1),
        KeyCode::Down => {
            let last = app.marketplaces.len().saturating_sub(1);
            app.marketplaces_index = (app.marketplaces_index + 1).min(last);
        }
        KeyCode::Char('a') => app.marketplace_start_add(),
        KeyCode::Enter => app.marketplace_enter_remove(),
        KeyCode::Char(c) => {
            if let Some(tab) = tab_for_digit(c) {
                app.set_plugins_tab(tab);
            }
        }
        KeyCode::Esc => app.overlay = Overlay::None,
        _ => {}
    }
}

/// Цифра `1`–`4` → таб по позиции в баре (быстрый прыжок вне Каталога).
fn tab_for_digit(c: char) -> Option<PluginsTab> {
    let index = c.to_digit(10)?.checked_sub(1)? as usize;
    PluginsTab::ALL.get(index).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::keytest::*;

    fn market(provider: Provider, name: &str) -> Marketplace {
        Marketplace {
            provider,
            name: name.to_string(),
            source: format!("src/{name}"),
        }
    }

    /// Tab листает табы по кругу, Shift+Tab — назад; панель при этом не закрывается.
    #[test]
    fn tab_cycles_through_tabs() {
        let mut app = app_for_keys();
        app.overlay = Overlay::Plugins;
        app.plugins_tab = PluginsTab::Overview;

        handle_plugins_key(&mut app, key(KeyCode::Tab));
        assert_eq!(app.plugins_tab, PluginsTab::Installed);
        handle_plugins_key(&mut app, key(KeyCode::Tab));
        assert_eq!(app.plugins_tab, PluginsTab::Catalog);
        handle_plugins_key(&mut app, key(KeyCode::Tab));
        assert_eq!(app.plugins_tab, PluginsTab::Sources);
        handle_plugins_key(&mut app, key(KeyCode::Tab));
        assert_eq!(app.plugins_tab, PluginsTab::Overview, "по кругу");
        assert_eq!(app.overlay, Overlay::Plugins, "панель осталась открытой");

        handle_plugins_key(&mut app, key(KeyCode::BackTab));
        assert_eq!(app.plugins_tab, PluginsTab::Sources, "Shift+Tab — назад");
    }

    /// ←/→ в списковом табе переключают провайдера (иначе codex тонет под каталогом claude).
    #[test]
    fn left_right_toggles_provider_in_list_tabs() {
        let mut app = app_for_keys();
        app.overlay = Overlay::Plugins;
        app.plugins_tab = PluginsTab::Catalog;
        assert_eq!(
            app.plugins_provider,
            Provider::Claude,
            "по умолчанию Claude"
        );

        handle_plugins_key(&mut app, key(KeyCode::Right));
        assert_eq!(
            app.plugins_provider,
            Provider::Codex,
            "→ переключил на Codex"
        );
        handle_plugins_key(&mut app, key(KeyCode::Left));
        assert_eq!(app.plugins_provider, Provider::Claude, "← вернул Claude");
    }

    /// `a` открывает ввод адреса; печать его наполняет; Tab меняет провайдера (а не выходит);
    /// Backspace стирает; Esc закрывает ввод, оставаясь в режиме источников.
    #[test]
    fn marketplace_add_input_types_toggles_provider_and_cancels() {
        let mut app = app_for_keys();
        app.overlay = Overlay::Plugins;
        app.plugins_tab = PluginsTab::Sources;
        app.marketplaces = vec![market(Provider::Claude, "official")];
        app.marketplaces_index = 0;

        handle_plugins_key(&mut app, key(KeyCode::Char('a')));
        let add = app.marketplace_input.as_ref().expect("a открыл ввод");
        assert_eq!(
            add.provider,
            Provider::Claude,
            "цель — провайдер выбранного"
        );

        handle_plugins_key(&mut app, key(KeyCode::Char('x')));
        handle_plugins_key(&mut app, key(KeyCode::Char('y')));
        assert_eq!(
            app.marketplace_input.as_ref().unwrap().source,
            "xy",
            "печать наполнила адрес"
        );

        handle_plugins_key(&mut app, key(KeyCode::Tab));
        assert_eq!(
            app.marketplace_input.as_ref().unwrap().provider,
            Provider::Codex,
            "Tab в вводе сменил провайдера, а не вышел из режима"
        );

        handle_plugins_key(&mut app, key(KeyCode::Backspace));
        assert_eq!(app.marketplace_input.as_ref().unwrap().source, "x");

        handle_plugins_key(&mut app, key(KeyCode::Esc));
        assert!(app.marketplace_input.is_none(), "Esc закрыл ввод");
        assert_eq!(app.plugins_tab, PluginsTab::Sources, "но из таба не вышел");
    }

    /// Enter на источнике просит подтверждения удаления; Esc отменяет его, следующий Esc
    /// (уже без подтверждения) закрывает панель.
    #[test]
    fn marketplace_enter_confirms_remove_then_esc_cancels_and_closes() {
        let mut app = app_for_keys();
        app.overlay = Overlay::Plugins;
        app.plugins_tab = PluginsTab::Sources;
        app.run_hooks.spawn = |_tx, _body| {};
        app.marketplaces = vec![market(Provider::Codex, "openai-bundled")];
        app.marketplaces_index = 0;

        handle_plugins_key(&mut app, key(KeyCode::Enter));
        let confirm = app
            .marketplace_confirm
            .as_ref()
            .expect("Enter → подтверждение");
        assert_eq!(confirm.name, "openai-bundled");

        handle_plugins_key(&mut app, key(KeyCode::Esc));
        assert!(
            app.marketplace_confirm.is_none(),
            "Esc отменил подтверждение"
        );
        assert_eq!(
            app.plugins_tab,
            PluginsTab::Sources,
            "остались в источниках"
        );

        handle_plugins_key(&mut app, key(KeyCode::Esc));
        assert_eq!(app.overlay, Overlay::None, "второй Esc закрыл панель");
    }
}
