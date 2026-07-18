use super::*;

/// Каталог конфигов claude по умолчанию: `~/.claude`. В тестах поле `App::claude_home`
/// подменяется на временный каталог, чтобы не читать реальный пользовательский (урок BUG-006).
pub(crate) fn default_claude_home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(".claude")
}

/// Читает три конфига claude из `claude_home` и разбирает в записи. Отсутствующий файл →
/// пустая строка → мягкий разбор (панель покажет, что смогла, без паники).
pub(crate) fn load_claude_plugins(claude_home: &Path) -> Vec<PluginEntry> {
    let plugins_dir = claude_home.join("plugins");
    let catalog =
        fs::read_to_string(plugins_dir.join("plugin-catalog-cache.json")).unwrap_or_default();
    let installed =
        fs::read_to_string(plugins_dir.join("installed_plugins.json")).unwrap_or_default();
    let settings = fs::read_to_string(claude_home.join("settings.json")).unwrap_or_default();

    // Официальный каталог-кэш + плагины КАЖДОГО стороннего маркетплейса из его манифеста: кэш их
    // не содержит, и без этого сторонние источники в Каталоге не видны. Дедуп по qualified_name
    // (официальный маркетплейс есть и в кэше, и в манифесте — берём из кэша, он с версиями).
    let mut plugins = parse_claude_plugins(&catalog, &installed, &settings);
    let mut seen: std::collections::HashSet<String> =
        plugins.iter().map(|p| p.qualified_name()).collect();
    for (marketplace, manifest) in read_marketplace_manifests(claude_home) {
        for entry in parse_marketplace_plugins(&manifest, &marketplace, &installed, &settings) {
            if seen.insert(entry.qualified_name()) {
                plugins.push(entry);
            }
        }
    }
    sort_plugins(&mut plugins);
    plugins
}

/// Читает манифесты всех сторонних маркетплейсов: `(имя_каталога, содержимое marketplace.json)`.
/// Каталога `plugins/marketplaces/` нет → пусто. Имя каталога = имя маркетплейса для установки.
fn read_marketplace_manifests(claude_home: &Path) -> Vec<(String, String)> {
    let dir = claude_home.join("plugins").join("marketplaces");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if let Ok(manifest) =
            fs::read_to_string(path.join(".claude-plugin").join("marketplace.json"))
        {
            out.push((name.to_string(), manifest));
        }
    }
    out
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

/// Читает `known_marketplaces.json` claude из `claude_home` и разбирает в список источников.
/// Отсутствующий файл → пустая строка → пустой список (без паники).
pub(crate) fn load_claude_marketplaces(claude_home: &Path) -> Vec<Marketplace> {
    let known = fs::read_to_string(claude_home.join("plugins").join("known_marketplaces.json"))
        .unwrap_or_default();
    parse_claude_marketplaces(&known)
}

/// Читает описания плагинов claude для области деталей: официальный каталог-кэш + манифесты
/// сторонних маркетплейсов. Официальные описания приоритетнее (не перекрываются манифестом).
pub(crate) fn load_claude_plugin_details(
    claude_home: &Path,
) -> std::collections::BTreeMap<String, PluginDetail> {
    let catalog = fs::read_to_string(
        claude_home
            .join("plugins")
            .join("plugin-catalog-cache.json"),
    )
    .unwrap_or_default();
    let mut details = parse_claude_plugin_details(&catalog);
    for (marketplace, manifest) in read_marketplace_manifests(claude_home) {
        for (key, detail) in parse_marketplace_details(&manifest, &marketplace) {
            details.entry(key).or_insert(detail);
        }
    }
    details
}

/// Множество плагинов claude с доступным обновлением (installed-версия ≠ catalog). Оба файла
/// отсутствуют → пусто.
pub(crate) fn load_claude_plugin_updates(claude_home: &Path) -> std::collections::BTreeSet<String> {
    let plugins_dir = claude_home.join("plugins");
    let catalog =
        fs::read_to_string(plugins_dir.join("plugin-catalog-cache.json")).unwrap_or_default();
    let installed =
        fs::read_to_string(plugins_dir.join("installed_plugins.json")).unwrap_or_default();
    parse_claude_plugin_updates(&catalog, &installed)
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

/// Отложенное действие над плагином, ждущее подтверждения (установка/удаление меняют окружение).
pub(crate) struct PendingPluginAction {
    pub(crate) action: PluginAction,
    pub(crate) entry: PluginEntry,
}

/// Открытый ввод адреса нового marketplace-источника: целевой провайдер (добавление идёт именно
/// в него — Tab в строке ввода переключает Claude ⇄ Codex) и набираемый адрес.
pub(crate) struct MarketplaceAdd {
    pub(crate) provider: Provider,
    pub(crate) source: String,
}

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

    /// Приём догруженного codex-списка: добавляем к уже показанному claude, снимаем флаг. Если
    /// действие было над codex-плагином — теперь он в списке, возвращаем на него курсор (финально).
    pub(crate) fn plugins_loaded(&mut self, mut codex: Vec<PluginEntry>) {
        self.plugins.append(&mut codex);
        self.plugins_loading = false;
        if self.plugins_reselect.is_some() {
            self.restore_plugins_selection();
            self.plugins_reselect = None;
        }
    }

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

    /// Выбранный в панели плагин (по курсору в отфильтрованном списке).
    fn selected_plugin(&self) -> Option<PluginEntry> {
        self.filtered_plugins()
            .get(self.plugins_index)
            .map(|p| (*p).clone())
    }

    /// Клавиша `Enter`: доступный плагин → установить, установленный → удалить (через
    /// подтверждение — оба необратимо-затратны).
    pub(crate) fn plugin_enter(&mut self) {
        let Some(entry) = self.selected_plugin() else {
            return;
        };
        let action = if entry.installed {
            PluginAction::Uninstall
        } else {
            PluginAction::Install
        };
        // Обе ветки необратимы → всегда через подтверждение.
        self.plugins_confirm = Some(PendingPluginAction { action, entry });
    }

    /// Вкл/выкл выбранного (обратимо, без подтверждения). На доступном (не установлен) — no-op.
    pub(crate) fn plugin_toggle(&mut self) {
        let Some(entry) = self.selected_plugin() else {
            return;
        };
        if !entry.installed {
            return;
        }
        let action = if entry.enabled {
            PluginAction::Disable
        } else {
            PluginAction::Enable
        };
        self.run_plugin_action(action, &entry);
    }

    /// Обновить выбранный установленный плагин (без подтверждения).
    pub(crate) fn plugin_update(&mut self) {
        if let Some(entry) = self.selected_plugin() {
            if entry.installed {
                self.run_plugin_action(PluginAction::Update, &entry);
            }
        }
    }

    /// Подтвердить отложенное действие (Enter в строке подтверждения).
    pub(crate) fn confirm_plugin_action(&mut self) {
        if let Some(pending) = self.plugins_confirm.take() {
            self.run_plugin_action(pending.action, &pending.entry);
        }
    }

    pub(crate) fn cancel_plugin_action(&mut self) {
        self.plugins_confirm = None;
    }

    /// Спавнит команду действия провайдера; по завершении шлёт `PluginActionDone` → refresh.
    fn run_plugin_action(&mut self, action: PluginAction, entry: &PluginEntry) {
        // Запоминаем плагин, чтобы после refresh вернуть курсор на него (вкл/выкл его не двигают).
        self.plugins_reselect = Some(entry.qualified_name());
        let mut cmd = match entry.provider {
            Provider::Claude => claude_action_cmd(action, entry),
            Provider::Codex => codex_action_cmd(action, entry),
        };
        self.status = self
            .lang
            .choose("плагин: выполняю…", "plugin: working…")
            .to_string();
        let tx = self.tx.clone();
        (self.run_hooks.spawn)(
            self.tx.clone(),
            Box::new(move || {
                let _ = cmd.output();
                let _ = tx.send(WorkerEvent::PluginActionDone);
            }),
        );
    }

    /// Действие завершено — перезагружаем список (статусы install/enabled могли измениться) и
    /// возвращаем курсор на тот же плагин (claude уже перезагружен; codex догрузится событием).
    pub(crate) fn plugin_action_done(&mut self) {
        self.plugins = load_claude_plugins(&self.claude_home);
        self.plugin_details = load_claude_plugin_details(&self.claude_home);
        self.plugin_updates = load_claude_plugin_updates(&self.claude_home);
        self.plugins_loading = true;
        self.restore_plugins_selection();
        self.spawn_codex_plugins_load();
    }

    /// Возвращает курсор на плагин из `plugins_reselect`, если он ещё в текущем (таб+провайдер)
    /// списке; иначе прижимает индекс к длине (плагин удалён или сменил таб). Для codex цель
    /// найдётся только после `plugins_loaded` — до тех пор `plugins_reselect` сохраняется.
    fn restore_plugins_selection(&mut self) {
        if let Some(name) = self.plugins_reselect.clone() {
            if let Some(pos) = self
                .filtered_plugins()
                .iter()
                .position(|p| p.qualified_name() == name)
            {
                self.plugins_index = pos;
                self.plugins_reselect = None;
                return;
            }
        }
        let last = self.filtered_plugins().len().saturating_sub(1);
        self.plugins_index = self.plugins_index.min(last);
    }

    // ── Marketplace-источники (таб «Источники») ─────────────────────────────────────────────

    /// Claude-источники читаем синхронно из конфига, codex догружаем воркером (спавн CLI долгий).
    fn load_marketplaces(&mut self) {
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

/// Другой из двух провайдеров — для переключения цели добавления источника.
fn other_provider(provider: Provider) -> Provider {
    match provider {
        Provider::Claude => Provider::Codex,
        Provider::Codex => Provider::Claude,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop_spawn(_tx: Sender<WorkerEvent>, _body: Box<dyn FnOnce() + Send + 'static>) {}

    fn count_of(app: &App, provider: Provider) -> usize {
        app.plugins
            .iter()
            .filter(|p| p.provider == provider)
            .count()
    }

    /// App на временных путях + no-op спавн: панель не поднимает реальный codex и не читает
    /// настоящий `~/.claude`.
    fn plugins_app(claude_home: &Path) -> (App, PathBuf) {
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
    fn seed_claude_home(dir: &Path) {
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

    #[test]
    fn load_claude_plugins_reads_from_injected_home_not_real_dir() {
        let home = env::temp_dir().join(format!("clave-claude-home-{}", std::process::id()));
        let _ = fs::create_dir_all(&home);
        seed_claude_home(&home);

        let plugins = load_claude_plugins(&home);
        assert_eq!(plugins.len(), 1, "прочитан ровно один плагин из фикстуры");
        assert_eq!(plugins[0].name, "context7");
        assert!(plugins[0].installed && plugins[0].enabled);

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn missing_configs_yield_empty_list_without_panic() {
        let home = env::temp_dir().join(format!("clave-empty-home-{}", std::process::id()));
        // Каталога нет вовсе — не паникуем, просто пусто.
        assert!(load_claude_plugins(&home).is_empty());
    }

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

    #[test]
    fn plugins_loaded_appends_codex_and_clears_loading() {
        let home = env::temp_dir().join(format!("clave-loaded-home-{}", std::process::id()));
        let _ = fs::create_dir_all(&home);
        seed_claude_home(&home);
        let (mut app, dir) = plugins_app(&home);
        app.open_plugins_panel();

        app.plugins_loaded(vec![PluginEntry {
            provider: Provider::Codex,
            name: "documents".to_string(),
            marketplace: "openai".to_string(),
            installed: true,
            enabled: true,
            version: None,
        }]);

        assert!(!app.plugins_loading, "флаг снят");
        assert_eq!(count_of(&app, Provider::Codex), 1);
        assert_eq!(count_of(&app, Provider::Claude), 1, "claude на месте");

        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn enter_on_installed_asks_to_uninstall_then_cancel_clears_it() {
        let home = env::temp_dir().join(format!("clave-act-home-{}", std::process::id()));
        let _ = fs::create_dir_all(&home);
        seed_claude_home(&home);
        let (mut app, dir) = plugins_app(&home);
        app.open_plugins_panel(); // context7 установлен
        app.plugins_tab = PluginsTab::Installed; // таб установленных — там он и виден

        app.plugins_index = 0;
        app.plugin_enter();
        let pending = app
            .plugins_confirm
            .as_ref()
            .expect("действие ждёт подтверждения");
        assert_eq!(
            pending.action,
            PluginAction::Uninstall,
            "установленный плагин → предложить удаление"
        );

        app.cancel_plugin_action();
        assert!(app.plugins_confirm.is_none(), "отмена сняла подтверждение");

        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn toggle_installed_runs_action_without_confirm() {
        let home = env::temp_dir().join(format!("clave-tog-home-{}", std::process::id()));
        let _ = fs::create_dir_all(&home);
        seed_claude_home(&home);
        let (mut app, dir) = plugins_app(&home);
        app.open_plugins_panel();
        app.plugins_tab = PluginsTab::Installed;
        app.plugins_index = 0;

        app.plugin_toggle(); // context7 включён → выключить, без подтверждения (обратимо)
        assert!(
            app.plugins_confirm.is_none(),
            "вкл/выкл не требует подтверждения"
        );
        assert!(
            app.status.contains("выполня") || app.status.contains("working"),
            "статус действия: {}",
            app.status
        );

        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&dir);
    }

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

    fn plugin_entry(provider: Provider, name: &str, installed: bool) -> PluginEntry {
        PluginEntry {
            provider,
            name: name.to_string(),
            marketplace: "m".into(),
            installed,
            enabled: installed,
            version: None,
        }
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

    #[test]
    fn action_returns_cursor_to_the_same_plugin_not_the_top() {
        let (mut app, dir) = plugins_app(&env::temp_dir());
        app.plugins_tab = PluginsTab::Installed;
        app.plugins = vec![
            plugin_entry(Provider::Claude, "aaa", true),
            plugin_entry(Provider::Claude, "bbb", true),
            plugin_entry(Provider::Claude, "ccc", true),
        ];
        // Действие над средним плагином запомнило его; refresh сбросил бы курсор на 0.
        app.plugins_reselect = Some("bbb@m".to_string());
        app.plugins_index = 0;
        app.restore_plugins_selection();
        assert_eq!(
            app.plugins_index, 1,
            "курсор вернулся на bbb, а не на первый"
        );
        assert!(app.plugins_reselect.is_none(), "цель снята после возврата");

        // Плагин ушёл из списка (удалён) → индекс прижат к длине, без паники.
        app.plugins = vec![plugin_entry(Provider::Claude, "aaa", true)];
        app.plugins_index = 5;
        app.plugins_reselect = Some("gone@m".to_string());
        app.restore_plugins_selection();
        assert_eq!(app.plugins_index, 0, "прижат к длине списка");

        let _ = fs::remove_dir_all(&dir);
    }

    fn codex_market(name: &str) -> Marketplace {
        Marketplace {
            provider: Provider::Codex,
            name: name.to_string(),
            source: format!("/local/{name}"),
        }
    }

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
