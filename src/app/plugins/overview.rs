use super::*;

/// Сводка для таба «Обзор»: сколько установлено (с разбивкой по провайдерам), доступно и
/// источников. Чистые числа — считаются из `app.plugins`/`app.marketplaces`, тестируемо.
pub(crate) struct PluginsOverview {
    pub(crate) installed: usize,
    pub(crate) claude_installed: usize,
    pub(crate) codex_installed: usize,
    pub(crate) available: usize,
    pub(crate) sources: usize,
}

/// Строки «Обзора» по порядку: клик (Enter) уводит в соответствующий таб.
pub(crate) const OVERVIEW_ROWS: [PluginsTab; 3] = [
    PluginsTab::Installed,
    PluginsTab::Catalog,
    PluginsTab::Sources,
];

impl App {
    /// Сводка для «Обзора»: числа установленного/доступного/источников.
    pub(crate) fn plugins_overview(&self) -> PluginsOverview {
        let installed: Vec<&PluginEntry> = self.plugins.iter().filter(|p| p.installed).collect();
        PluginsOverview {
            installed: installed.len(),
            claude_installed: installed
                .iter()
                .filter(|p| p.provider == Provider::Claude)
                .count(),
            codex_installed: installed
                .iter()
                .filter(|p| p.provider == Provider::Codex)
                .count(),
            available: self.plugins.iter().filter(|p| !p.installed).count(),
            sources: self.marketplaces.len(),
        }
    }

    /// Переключить таб (`Tab`/`Shift+Tab`/цифра/Enter в Обзоре). Курсоры сбрасываем, а ввод и
    /// подтверждения не тащим между табами (иначе строка ввода источника всплыла бы в Каталоге).
    pub(crate) fn set_plugins_tab(&mut self, tab: PluginsTab) {
        self.plugins_tab = tab;
        self.plugins_index = 0;
        self.marketplaces_index = 0;
        self.marketplace_input = None;
        self.marketplace_confirm = None;
        self.plugins_confirm = None;
        // Поиск осмыслен только в Каталоге — уходя из него, сбрасываем.
        if tab != PluginsTab::Catalog {
            self.plugins_query.clear();
        }
    }

    pub(crate) fn plugins_tab_next(&mut self) {
        self.set_plugins_tab(self.plugins_tab.next());
    }

    pub(crate) fn plugins_tab_prev(&mut self) {
        self.set_plugins_tab(self.plugins_tab.prev());
    }

    /// Enter в «Обзоре» — прыжок в таб выбранной строки.
    pub(crate) fn overview_enter(&mut self) {
        if let Some(tab) = OVERVIEW_ROWS.get(self.overview_index) {
            self.set_plugins_tab(*tab);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::plugins::testkit::*;

    #[test]
    fn plugins_overview_counts_by_provider_and_availability() {
        let (mut app, dir) = plugins_app(&env::temp_dir());
        app.plugins = vec![
            plugin_entry(Provider::Claude, "a", true),
            plugin_entry(Provider::Claude, "b", true),
            plugin_entry(Provider::Codex, "c", true),
            plugin_entry(Provider::Claude, "d", false),
            plugin_entry(Provider::Codex, "e", false),
        ];
        app.marketplaces = vec![codex_market("m1"), codex_market("m2")];

        let overview = app.plugins_overview();
        assert_eq!(overview.installed, 3);
        assert_eq!(overview.claude_installed, 2);
        assert_eq!(overview.codex_installed, 1);
        assert_eq!(overview.available, 2);
        assert_eq!(overview.sources, 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn overview_enter_jumps_to_selected_row_tab() {
        let (mut app, dir) = plugins_app(&env::temp_dir());
        // Строки Обзора: 0 Установленные · 1 Каталог · 2 Источники.
        app.overview_index = 1;
        app.overview_enter();
        assert_eq!(app.plugins_tab, PluginsTab::Catalog);

        let _ = fs::remove_dir_all(&dir);
    }
}
