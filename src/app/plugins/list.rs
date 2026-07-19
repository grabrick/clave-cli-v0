use super::*;

impl App {
    /// Плагины активного таба: «Установленные» → установленные, «Каталог» → доступные — и только
    /// текущего провайдера (`←/→` переключает; каталог claude огромен и иначе прячет codex). В
    /// Каталоге — ещё инкрементальный поиск по имени. Обзор/Источники список не показывают.
    pub(crate) fn filtered_plugins(&self) -> Vec<&PluginEntry> {
        let installed_wanted = match self.plugins_tab {
            PluginsTab::Installed => true,
            PluginsTab::Catalog => false,
            PluginsTab::Overview | PluginsTab::Sources => return Vec::new(),
        };
        let base: Vec<&PluginEntry> = self
            .plugins
            .iter()
            .filter(|p| p.installed == installed_wanted && p.provider == self.plugins_provider)
            .collect();
        // Поиск живёт только в Каталоге (там сотни плагинов); Установленных единицы.
        if self.plugins_tab == PluginsTab::Catalog && !self.plugins_query.is_empty() {
            let needle = self.plugins_query.to_lowercase();
            return base
                .into_iter()
                .filter(|p| p.name.to_lowercase().contains(&needle))
                .collect();
        }
        base
    }

    /// `←/→` в списковых табах: переключить показываемого провайдера (Claude ⇄ Codex).
    pub(crate) fn toggle_plugins_provider(&mut self) {
        self.plugins_provider = other_provider(self.plugins_provider);
        self.plugins_index = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::plugins::testkit::*;

    #[test]
    fn search_filters_plugins_by_name() {
        let (mut app, dir) = plugins_app(&env::temp_dir());
        app.plugins = vec![
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
        ];
        app.plugins_tab = PluginsTab::Catalog; // поиск живёт в Каталоге (доступные)
        app.plugins_provider = Provider::Codex; // documents — codex-доступный
        app.plugins_query = "doc".into();
        let filtered = app.filtered_plugins();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "documents");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn filtered_plugins_splits_installed_and_available_by_tab() {
        let (mut app, dir) = plugins_app(&env::temp_dir());
        app.plugins = vec![
            plugin_entry(Provider::Claude, "inst", true),
            plugin_entry(Provider::Codex, "avail", false),
        ];

        // Установленные + провайдер Claude (по умолчанию) → только claude-installed.
        app.plugins_tab = PluginsTab::Installed;
        let inst = app.filtered_plugins();
        assert_eq!(
            inst.len(),
            1,
            "Установленные — только installed текущего провайдера"
        );
        assert_eq!(inst[0].name, "inst");

        // Каталог + Claude → пусто (доступный-то codex); ←/→ показывает его.
        app.plugins_tab = PluginsTab::Catalog;
        assert!(
            app.filtered_plugins().is_empty(),
            "codex-доступный не виден под Claude"
        );
        app.toggle_plugins_provider(); // → Codex
        let avail = app.filtered_plugins();
        assert_eq!(avail.len(), 1, "Каталог+Codex — доступный codex");
        assert_eq!(avail[0].name, "avail");

        // Обзор и Источники список плагинов не показывают.
        app.plugins_tab = PluginsTab::Overview;
        assert!(app.filtered_plugins().is_empty());
        app.plugins_tab = PluginsTab::Sources;
        assert!(app.filtered_plugins().is_empty());

        let _ = fs::remove_dir_all(&dir);
    }
}
