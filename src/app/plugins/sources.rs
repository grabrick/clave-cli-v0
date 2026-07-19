use super::*;

// ── Marketplace-источники (таб «Источники») ─────────────────────────────────────────────

/// Открытый ввод адреса нового marketplace-источника: целевой провайдер (добавление идёт именно
/// в него — Tab в строке ввода переключает Claude ⇄ Codex) и набираемый адрес.
pub(crate) struct MarketplaceAdd {
    pub(crate) provider: Provider,
    pub(crate) source: String,
}

/// Спавнит `codex plugin marketplace list --json`, возвращает stdout (пусто при ошибке —
/// секция codex покажется пустой, без паники).
fn run_codex_marketplace_list() -> String {
    Command::new(codex_binary())
        .args(["plugin", "marketplace", "list", "--json"])
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default()
}

impl App {
    /// Claude-источники читаем синхронно из конфига, codex догружаем воркером (спавн CLI долгий).
    pub(crate) fn load_marketplaces(&mut self) {
        self.marketplaces = load_claude_marketplaces(&self.claude_home);
        self.marketplaces_loading = true;
        self.spawn_codex_marketplaces_load();
    }

    fn spawn_codex_marketplaces_load(&mut self) {
        let tx = self.tx.clone();
        (self.run_hooks.spawn)(
            self.tx.clone(),
            Box::new(move || {
                let markets = parse_codex_marketplaces(&run_codex_marketplace_list());
                let _ = tx.send(WorkerEvent::MarketplacesLoaded(markets));
            }),
        );
    }

    /// Приём догруженных codex-источников: добавляем к claude, снимаем флаг догрузки.
    pub(crate) fn marketplaces_loaded(&mut self, mut codex: Vec<Marketplace>) {
        self.marketplaces.append(&mut codex);
        self.marketplaces_loading = false;
    }

    /// Выбранный источник (по курсору в общем списке обоих провайдеров).
    fn selected_marketplace(&self) -> Option<Marketplace> {
        self.marketplaces.get(self.marketplaces_index).cloned()
    }

    /// `a`: открыть ввод адреса нового источника. Целевой провайдер — у выбранного источника
    /// (или Claude, если список пуст); в строке ввода его можно переключить Tab-ом.
    pub(crate) fn marketplace_start_add(&mut self) {
        let provider = self
            .selected_marketplace()
            .map(|m| m.provider)
            .unwrap_or(Provider::Claude);
        self.marketplace_input = Some(MarketplaceAdd {
            provider,
            source: String::new(),
        });
    }

    /// Tab в строке ввода: переключить, в какого провайдера добавляем источник.
    pub(crate) fn marketplace_toggle_add_provider(&mut self) {
        if let Some(add) = &mut self.marketplace_input {
            add.provider = other_provider(add.provider);
        }
    }

    pub(crate) fn marketplace_input_push(&mut self, c: char) {
        if let Some(add) = &mut self.marketplace_input {
            add.source.push(c);
        }
    }

    pub(crate) fn marketplace_input_backspace(&mut self) {
        if let Some(add) = &mut self.marketplace_input {
            add.source.pop();
        }
    }

    pub(crate) fn marketplace_cancel_input(&mut self) {
        self.marketplace_input = None;
    }

    /// Enter в строке ввода: добавить источник командой целевого провайдера. Пустой адрес —
    /// просто закрыть ввод (добавлять нечего).
    pub(crate) fn marketplace_submit_add(&mut self) {
        let Some(add) = self.marketplace_input.take() else {
            return;
        };
        let source = add.source.trim().to_string();
        if source.is_empty() {
            return;
        }
        let cmd = match add.provider {
            Provider::Claude => claude_marketplace_add_cmd(&source),
            Provider::Codex => codex_marketplace_add_cmd(&source),
        };
        self.run_marketplace_action(cmd);
    }

    /// Enter на источнике: удаление — через подтверждение (необратимо-затратно).
    pub(crate) fn marketplace_enter_remove(&mut self) {
        if let Some(market) = self.selected_marketplace() {
            self.marketplace_confirm = Some(market);
        }
    }

    pub(crate) fn confirm_marketplace_remove(&mut self) {
        let Some(market) = self.marketplace_confirm.take() else {
            return;
        };
        let cmd = match market.provider {
            Provider::Claude => claude_marketplace_remove_cmd(&market.name),
            Provider::Codex => codex_marketplace_remove_cmd(&market.name),
        };
        self.run_marketplace_action(cmd);
    }

    pub(crate) fn cancel_marketplace_remove(&mut self) {
        self.marketplace_confirm = None;
    }

    /// Спавнит команду add/remove источника; по завершении шлёт `MarketplaceActionDone` → refresh.
    fn run_marketplace_action(&mut self, mut cmd: Command) {
        self.status = self
            .lang
            .choose("источник: выполняю…", "marketplace: working…")
            .to_string();
        let tx = self.tx.clone();
        (self.run_hooks.spawn)(
            self.tx.clone(),
            Box::new(move || {
                let _ = cmd.output();
                let _ = tx.send(WorkerEvent::MarketplaceActionDone);
            }),
        );
    }

    /// Действие с источником завершено — перезагружаем список источников.
    pub(crate) fn marketplace_action_done(&mut self) {
        self.marketplaces_index = 0;
        self.load_marketplaces();
        // Источник добавлен/удалён — его плагины появились/ушли: обновляем и Каталог.
        self.plugins = load_claude_plugins(&self.claude_home);
        self.plugin_details = load_claude_plugin_details(&self.claude_home);
        self.plugin_updates = load_claude_plugin_updates(&self.claude_home);
        self.plugins_loading = true;
        self.spawn_codex_plugins_load();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::plugins::testkit::*;

    #[test]
    fn open_reads_marketplaces_and_tab_navigation_cycles() {
        let home = env::temp_dir().join(format!("clave-mkt-home-{}", std::process::id()));
        let _ = fs::create_dir_all(&home);
        seed_claude_home(&home);
        let (mut app, dir) = plugins_app(&home);

        app.open_plugins_panel();

        // Источники читаются СРАЗУ при открытии — «Обзор» показывает их число.
        assert!(app.marketplaces_loading, "codex-источники ещё догружаются");
        assert_eq!(
            app.marketplaces.len(),
            1,
            "claude-источник прочитан при открытии"
        );
        assert_eq!(app.marketplaces[0].name, "official");
        assert_eq!(app.marketplaces[0].provider, Provider::Claude);
        assert_eq!(app.plugins_tab, PluginsTab::Overview, "вход — на Обзоре");

        // Tab/Shift+Tab листают бар по кругу.
        app.plugins_tab_next();
        assert_eq!(app.plugins_tab, PluginsTab::Installed);
        app.plugins_tab_prev();
        assert_eq!(app.plugins_tab, PluginsTab::Overview);
        app.set_plugins_tab(PluginsTab::Sources);
        assert_eq!(app.plugins_tab, PluginsTab::Sources);

        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn marketplaces_loaded_appends_codex_and_clears_loading() {
        let home = env::temp_dir().join(format!("clave-mkt-load-{}", std::process::id()));
        let _ = fs::create_dir_all(&home);
        seed_claude_home(&home);
        let (mut app, dir) = plugins_app(&home);
        app.open_plugins_panel();

        app.marketplaces_loaded(vec![codex_market("openai-bundled")]);

        assert!(!app.marketplaces_loading, "флаг догрузки снят");
        assert_eq!(app.marketplaces.len(), 2, "claude + codex");
        assert!(
            app.marketplaces
                .iter()
                .any(|m| m.provider == Provider::Codex),
            "codex-источник добавлен"
        );

        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn start_add_targets_selected_provider_and_tab_toggles_it() {
        let (mut app, dir) = plugins_app(&env::temp_dir());
        app.marketplaces = vec![codex_market("openai-bundled")];
        app.marketplaces_index = 0;

        app.marketplace_start_add();
        let add = app.marketplace_input.as_ref().expect("ввод открыт");
        assert_eq!(
            add.provider,
            Provider::Codex,
            "цель — провайдер выбранного источника"
        );
        assert!(add.source.is_empty(), "адрес начинается пустым");

        app.marketplace_toggle_add_provider();
        assert_eq!(
            app.marketplace_input.as_ref().unwrap().provider,
            Provider::Claude,
            "Tab переключил цель на другого провайдера"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn start_add_on_empty_list_defaults_to_claude() {
        let (mut app, dir) = plugins_app(&env::temp_dir());
        app.marketplaces.clear();

        app.marketplace_start_add();

        assert_eq!(
            app.marketplace_input
                .as_ref()
                .expect("ввод открыт")
                .provider,
            Provider::Claude,
            "без выбора цель по умолчанию — Claude"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn submit_empty_source_closes_input_without_running() {
        let (mut app, dir) = plugins_app(&env::temp_dir());
        app.marketplace_input = Some(MarketplaceAdd {
            provider: Provider::Claude,
            source: "   ".to_string(),
        });

        app.marketplace_submit_add();

        assert!(app.marketplace_input.is_none(), "пустой ввод закрылся");
        assert!(
            !app.status.contains("выполня"),
            "команда не запускалась: {}",
            app.status
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn submit_nonempty_source_runs_action() {
        let (mut app, dir) = plugins_app(&env::temp_dir());
        app.marketplace_input = Some(MarketplaceAdd {
            provider: Provider::Claude,
            source: "owner/repo".to_string(),
        });

        app.marketplace_submit_add();

        assert!(app.marketplace_input.is_none(), "ввод снят после запуска");
        assert!(
            app.status.contains("выполня") || app.status.contains("working"),
            "статус действия: {}",
            app.status
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn enter_remove_asks_confirmation_then_cancel_clears_it() {
        let (mut app, dir) = plugins_app(&env::temp_dir());
        app.marketplaces = vec![codex_market("openai-bundled")];
        app.marketplaces_index = 0;

        app.marketplace_enter_remove();
        let market = app
            .marketplace_confirm
            .as_ref()
            .expect("удаление ждёт подтверждения");
        assert_eq!(market.name, "openai-bundled");

        app.cancel_marketplace_remove();
        assert!(
            app.marketplace_confirm.is_none(),
            "отмена сняла подтверждение"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn confirm_remove_runs_action_and_clears_confirm() {
        let (mut app, dir) = plugins_app(&env::temp_dir());
        app.marketplace_confirm = Some(codex_market("openai-bundled"));

        app.confirm_marketplace_remove();

        assert!(app.marketplace_confirm.is_none(), "подтверждение снято");
        assert!(
            app.status.contains("выполня") || app.status.contains("working"),
            "статус действия: {}",
            app.status
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
