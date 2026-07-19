use super::*;

mod actions;
mod list;
mod loaders;
mod overview;
mod sources;
pub(crate) use actions::*;
pub(crate) use loaders::*;
pub(crate) use overview::*;
pub(crate) use sources::*;

/// Таб панели `/plugins`. Вход — на «Обзор» (сводка), чтобы не вываливать сотни доступных
/// плагинов сразу; список смотрят в «Установленных» и «Каталоге», источники — в своём табе.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PluginsTab {
    Overview,
    Installed,
    Catalog,
    Sources,
}

impl PluginsTab {
    pub(crate) const ALL: [PluginsTab; 4] = [
        PluginsTab::Overview,
        PluginsTab::Installed,
        PluginsTab::Catalog,
        PluginsTab::Sources,
    ];

    /// Следующий/предыдущий таб по кругу — для `Tab`/`Shift+Tab`.
    pub(crate) fn next(self) -> PluginsTab {
        let index = Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    pub(crate) fn prev(self) -> PluginsTab {
        let index = Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0);
        Self::ALL[(index + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    pub(crate) fn label(self, lang: Language) -> &'static str {
        match self {
            PluginsTab::Overview => lang.choose("Обзор", "Overview"),
            PluginsTab::Installed => lang.choose("Установленные", "Installed"),
            PluginsTab::Catalog => lang.choose("Каталог", "Catalog"),
            PluginsTab::Sources => lang.choose("Источники", "Sources"),
        }
    }
}

impl App {
    /// Открывает панель плагинов. Claude-список читается СИНХРОННО из конфигов (мгновенно),
    /// codex-список догружается АСИНХРОННО воркером (спавн CLI занимает время).
    pub(crate) fn open_plugins_panel(&mut self) {
        self.plugins = load_claude_plugins(&self.claude_home);
        self.plugin_details = load_claude_plugin_details(&self.claude_home);
        self.plugin_updates = load_claude_plugin_updates(&self.claude_home);
        self.plugins_index = 0;
        self.plugins_reselect = None;
        self.plugins_loading = true;
        // Панель всегда открывается на «Обзоре», без залипшего таба/ввода с прошлого раза.
        self.plugins_tab = PluginsTab::Overview;
        self.overview_index = 0;
        self.plugins_provider = Provider::Claude;
        self.plugins_query.clear();
        self.marketplace_input = None;
        self.marketplace_confirm = None;
        self.plugins_confirm = None;
        self.overlay = Overlay::Plugins;
        self.status = self.lang.choose("плагины", "plugins").to_string();
        self.spawn_codex_plugins_load();
        // Источники грузим сразу — «Обзор» показывает их число, не дожидаясь входа в таб.
        self.load_marketplaces();
    }

    /// Асинхронная догрузка codex: спавн через раннер-хук (в тестах подменяется no-op),
    /// результат приходит как `WorkerEvent::PluginsLoaded`.
    fn spawn_codex_plugins_load(&mut self) {
        let tx = self.tx.clone();
        (self.run_hooks.spawn)(
            self.tx.clone(),
            Box::new(move || {
                let plugins = parse_codex_plugins(&run_codex_plugin_list());
                let _ = tx.send(WorkerEvent::PluginsLoaded(plugins));
            }),
        );
    }
}

/// Спавнит `codex plugin list --available --json`, возвращает stdout (или пусто при ошибке —
/// панель покажет секцию codex как «не удалось загрузить»).
fn run_codex_plugin_list() -> String {
    Command::new(codex_binary())
        .args(["plugin", "list", "--available", "--json"])
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default()
}

/// Другой из двух провайдеров — для переключения цели добавления источника.
fn other_provider(provider: Provider) -> Provider {
    match provider {
        Provider::Claude => Provider::Codex,
        Provider::Codex => Provider::Claude,
    }
}

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;

    fn noop_spawn(_tx: Sender<WorkerEvent>, _body: Box<dyn FnOnce() + Send + 'static>) {}

    pub(crate) fn count_of(app: &App, provider: Provider) -> usize {
        app.plugins
            .iter()
            .filter(|p| p.provider == provider)
            .count()
    }

    /// App на временных путях + no-op спавн: панель не поднимает реальный codex и не читает
    /// настоящий `~/.claude`.
    pub(crate) fn plugins_app(claude_home: &Path) -> (App, PathBuf) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let dir = env::temp_dir().join(format!(
            "clave-plugins-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::create_dir_all(&dir);
        let config = AppConfig {
            onboarding_done: true,
            ..AppConfig::default()
        };
        let mut app = App::from_config(
            config,
            dir.join("config.json"),
            dir.join("history"),
            dir.clone(),
        );
        app.onboarding = None;
        app.run_hooks.spawn = noop_spawn;
        app.claude_home = claude_home.to_path_buf();
        (app, dir)
    }

    /// Пишет claude-конфиги во временный `claude_home`.
    pub(crate) fn seed_claude_home(dir: &Path) {
        let plugins = dir.join("plugins");
        let _ = fs::create_dir_all(&plugins);
        let _ = fs::write(
            plugins.join("plugin-catalog-cache.json"),
            r#"{"catalog":{"plugins":{"context7@official":{"version":"1.2"}}}}"#,
        );
        let _ = fs::write(
            plugins.join("installed_plugins.json"),
            r#"{"version":2,"plugins":{"context7@official":[{"version":"1.2"}]}}"#,
        );
        let _ = fs::write(
            dir.join("settings.json"),
            r#"{"enabledPlugins":{"context7@official":true}}"#,
        );
        let _ = fs::write(
            plugins.join("known_marketplaces.json"),
            r#"{"official":{"source":{"source":"github","repo":"anthropics/official"}}}"#,
        );
    }

    pub(crate) fn plugin_entry(provider: Provider, name: &str, installed: bool) -> PluginEntry {
        PluginEntry {
            provider,
            name: name.to_string(),
            marketplace: "m".into(),
            installed,
            enabled: installed,
            version: None,
        }
    }

    pub(crate) fn codex_market(name: &str) -> Marketplace {
        Marketplace {
            provider: Provider::Codex,
            name: name.to_string(),
            source: format!("/local/{name}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

    #[test]
    fn open_plugins_panel_loads_claude_and_marks_loading() {
        let home = env::temp_dir().join(format!("clave-open-home-{}", std::process::id()));
        let _ = fs::create_dir_all(&home);
        seed_claude_home(&home);
        let (mut app, dir) = plugins_app(&home);
        // Осталось с прошлого открытия: панель обязана сбросить это и открыться на Обзоре.
        app.plugins_tab = PluginsTab::Sources;
        app.marketplace_input = Some(MarketplaceAdd {
            provider: Provider::Codex,
            source: "хвост".into(),
        });

        app.open_plugins_panel();

        assert_eq!(app.overlay, Overlay::Plugins);
        assert!(app.plugins_loading, "codex ещё догружается");
        assert!(
            app.plugins_tab == PluginsTab::Overview && app.marketplace_input.is_none(),
            "панель открывается на Обзоре, без залипшего таба/ввода источников"
        );
        assert_eq!(count_of(&app, Provider::Claude), 1, "claude уже виден");
        assert!(
            count_of(&app, Provider::Codex) == 0,
            "codex придёт событием"
        );

        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&dir);
    }
}
