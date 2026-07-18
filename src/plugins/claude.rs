use crate::prelude::*;
use crate::*;
use std::collections::{BTreeMap, BTreeSet};

/// Разбирает три конфига claude в унифицированные записи (гибрид-источник):
/// - `catalog_json` (`plugin-catalog-cache.json`) → ДОСТУПНЫЕ плагины — основа списка;
/// - `installed_json` (`installed_plugins.json`) → отмечает установленные и версию;
/// - `settings_json` (`settings.json` → `enabledPlugins`) → вкл/выкл.
///
/// Ключи везде вида `name@marketplace`. Любой битый/пустой вход обрабатывается мягко: что
/// удалось разобрать — попадёт в список, паники нет (тесты — на фикстурах, не на реальном `~/.claude`).
pub(crate) fn parse_claude_plugins(
    catalog_json: &str,
    installed_json: &str,
    settings_json: &str,
) -> Vec<PluginEntry> {
    let installed = claude_installed_versions(installed_json);
    let enabled = claude_enabled_set(settings_json);

    // Каталог доступных — основа. По каждому ключу решаем installed/enabled/version.
    let mut entries: BTreeMap<String, PluginEntry> = BTreeMap::new();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(catalog_json) {
        if let Some(plugins) = value
            .get("catalog")
            .and_then(|c| c.get("plugins"))
            .and_then(|p| p.as_object())
        {
            for (key, meta) in plugins {
                let version = meta
                    .get("version")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .or_else(|| installed.get(key).cloned());
                entries.insert(
                    key.clone(),
                    claude_entry(key, &installed, &enabled, version),
                );
            }
        }
    }

    // Установленные, которых нет в каталоге (например, из локального marketplace) — тоже в список.
    for (key, version) in &installed {
        entries
            .entry(key.clone())
            .or_insert_with(|| claude_entry(key, &installed, &enabled, Some(version.clone())));
    }

    let mut out: Vec<PluginEntry> = entries.into_values().collect();
    sort_plugins(&mut out);
    out
}

fn claude_entry(
    key: &str,
    installed: &BTreeMap<String, String>,
    enabled: &BTreeSet<String>,
    version: Option<String>,
) -> PluginEntry {
    let (name, marketplace) = split_qualified(key);
    PluginEntry {
        provider: Provider::Claude,
        name,
        marketplace,
        installed: installed.contains_key(key),
        enabled: enabled.contains(key),
        // «unknown» из конфига — не версия, а заглушка: прячем её.
        version: version.filter(|v| v != "unknown"),
    }
}

/// `name@marketplace` → (name, marketplace). Источник в имени плагина не встречается, поэтому
/// делим по ПОСЛЕДНЕМУ `@`. Без `@` — весь ключ имя, источник пуст.
fn split_qualified(key: &str) -> (String, String) {
    match key.rsplit_once('@') {
        Some((name, marketplace)) => (name.to_string(), marketplace.to_string()),
        None => (key.to_string(), String::new()),
    }
}

fn claude_installed_versions(json: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return out;
    };
    if let Some(plugins) = value.get("plugins").and_then(|p| p.as_object()) {
        for (key, installs) in plugins {
            // Значение — массив установок (scope user/project); версию берём из первой.
            let version = installs
                .as_array()
                .and_then(|a| a.first())
                .and_then(|i| i.get("version"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            out.insert(key.clone(), version);
        }
    }
    out
}

fn claude_enabled_set(json: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return out;
    };
    if let Some(enabled) = value.get("enabledPlugins").and_then(|e| e.as_object()) {
        for (key, on) in enabled {
            if on.as_bool().unwrap_or(false) {
                out.insert(key.clone());
            }
        }
    }
    out
}

/// Строит команду действия claude: `claude plugin <sub> <plugin@marketplace>`. Спавн — в слое
/// app; здесь только сборка `Command`, чтобы тесты проверяли аргументы без реальных процессов.
pub(crate) fn claude_action_cmd(action: PluginAction, entry: &PluginEntry) -> Command {
    let sub = match action {
        PluginAction::Install => "install",
        PluginAction::Uninstall => "uninstall",
        PluginAction::Enable => "enable",
        PluginAction::Disable => "disable",
        PluginAction::Update => "update",
    };
    let mut cmd = Command::new(claude_binary());
    cmd.args(["plugin", sub, &entry.qualified_name()]);
    cmd
}

/// Разбирает `known_marketplaces.json` claude в список источников. Формат — объект
/// `{ "<имя>": { "source": {...} } }`; адрес достаём из вложенного `source`
/// ([`claude_marketplace_source`]). Битый/не-объект → пусто, без паники.
pub(crate) fn parse_claude_marketplaces(known_json: &str) -> Vec<Marketplace> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(known_json) else {
        return Vec::new();
    };
    let Some(map) = value.as_object() else {
        return Vec::new();
    };
    map.iter()
        .map(|(name, meta)| Marketplace {
            provider: Provider::Claude,
            name: name.clone(),
            source: claude_marketplace_source(meta),
        })
        .collect()
}

/// Человекочитаемый адрес из объекта `source`: GitHub `repo` (`owner/name`), git-`url`,
/// локальный `path`, а если ничего из этого нет — тип источника (`github`/`git`), чтобы строка
/// не была пустой. Разные claude-версии кладут адрес в разные поля — поэтому перебор.
fn claude_marketplace_source(meta: &serde_json::Value) -> String {
    let src = meta.get("source");
    let field = |key| src.and_then(|s| s.get(key)).and_then(|v| v.as_str());
    field("repo")
        .or_else(|| field("url"))
        .or_else(|| field("path"))
        .or_else(|| field("source"))
        .unwrap_or("")
        .to_string()
}

/// `claude plugin marketplace add <источник>` — источник это URL, путь или GitHub `owner/repo`.
pub(crate) fn claude_marketplace_add_cmd(source: &str) -> Command {
    let mut cmd = Command::new(claude_binary());
    cmd.args(["plugin", "marketplace", "add", source]);
    cmd
}

/// `claude plugin marketplace remove <имя>` — удаление источника по его короткому имени.
pub(crate) fn claude_marketplace_remove_cmd(name: &str) -> Command {
    let mut cmd = Command::new(claude_binary());
    cmd.args(["plugin", "marketplace", "remove", name]);
    cmd
}

/// Разбирает описания плагинов из `plugin-catalog-cache.json` для области деталей панели: адрес у
/// каждой записи — вложенный `marketplace_entry` с `description`/`author`. Ключ карты — тот же
/// `name@marketplace`, что у [`parse_claude_plugins`], чтобы деталь искалась по `qualified_name`.
/// Записи без описания пропускаем (нечего показывать); битый JSON → пустая карта.
pub(crate) fn parse_claude_plugin_details(catalog_json: &str) -> BTreeMap<String, PluginDetail> {
    let mut out = BTreeMap::new();
    let Ok(value) = serde_json::from_str::<serde_json::Value>(catalog_json) else {
        return out;
    };
    let Some(plugins) = value
        .get("catalog")
        .and_then(|c| c.get("plugins"))
        .and_then(|p| p.as_object())
    else {
        return out;
    };
    for (key, meta) in plugins {
        let entry = meta.get("marketplace_entry");
        let description = entry
            .and_then(|e| e.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if description.is_empty() {
            continue;
        }
        // `author` — объект `{ "name", "email"? }`, а не строка: берём имя.
        let author = entry
            .and_then(|e| e.get("author"))
            .and_then(|a| a.get("name"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from);
        out.insert(
            key.clone(),
            PluginDetail {
                description,
                author,
            },
        );
    }
    out
}

/// Множество плагинов (`qualified_name`), у которых доступно обновление: установленная версия
/// ИЗВЕСТНА и отличается от версии в каталоге (каталог — последняя известная). «unknown»/пустая
/// версия или отсутствие версии в каталоге → не судим (в множество не попадает).
pub(crate) fn parse_claude_plugin_updates(
    catalog_json: &str,
    installed_json: &str,
) -> BTreeSet<String> {
    let installed = claude_installed_versions(installed_json);
    let mut out = BTreeSet::new();
    let Ok(value) = serde_json::from_str::<serde_json::Value>(catalog_json) else {
        return out;
    };
    let Some(plugins) = value
        .get("catalog")
        .and_then(|c| c.get("plugins"))
        .and_then(|p| p.as_object())
    else {
        return out;
    };
    for (key, meta) in plugins {
        let catalog_version = meta.get("version").and_then(|v| v.as_str()).unwrap_or("");
        let installed_version = installed.get(key).map(String::as_str).unwrap_or("");
        if installed_version.is_empty()
            || installed_version == "unknown"
            || catalog_version.is_empty()
        {
            continue;
        }
        if installed_version != catalog_version {
            out.insert(key.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd_args(cmd: &Command) -> Vec<String> {
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn action_cmd_builds_plugin_subcommands() {
        let entry = PluginEntry {
            provider: Provider::Claude,
            name: "context7".into(),
            marketplace: "official".into(),
            installed: true,
            enabled: true,
            version: None,
        };
        assert_eq!(
            cmd_args(&claude_action_cmd(PluginAction::Install, &entry)),
            ["plugin", "install", "context7@official"]
        );
        assert_eq!(
            cmd_args(&claude_action_cmd(PluginAction::Disable, &entry)),
            ["plugin", "disable", "context7@official"]
        );
    }

    // Фикстуры по РЕАЛЬНЫМ форматам конфигов claude.
    const CATALOG: &str = r#"{"catalog":{"plugins":{
        "context7@claude-plugins-official":{"version":"1.2","source":"git"},
        "42crunch-api-security-testing@claude-plugins-official":{"version":"0.9"}
    }}}"#;
    const INSTALLED: &str = r#"{"version":2,"plugins":{
        "context7@claude-plugins-official":[{"scope":"user","version":"1.2"}],
        "local-tool@my-mkt":[{"scope":"user","version":"unknown"}]
    }}"#;
    const SETTINGS: &str = r#"{"enabledPlugins":{
        "context7@claude-plugins-official":true
    }}"#;

    #[test]
    fn merges_catalog_installed_and_enabled() {
        let plugins = parse_claude_plugins(CATALOG, INSTALLED, SETTINGS);
        let by_name = |n: &str| plugins.iter().find(|p| p.name == n).cloned();

        // Установленный и включённый, версия из каталога.
        let ctx = by_name("context7").expect("context7 в списке");
        assert_eq!(ctx.provider, Provider::Claude);
        assert_eq!(ctx.marketplace, "claude-plugins-official");
        assert!(ctx.installed && ctx.enabled);
        assert_eq!(ctx.version.as_deref(), Some("1.2"));

        // Из каталога, но НЕ установлен → доступен, выключен.
        let crunch = by_name("42crunch-api-security-testing").expect("есть");
        assert!(!crunch.installed && !crunch.enabled);

        // Установлен, но НЕ в каталоге (локальный marketplace) → в списке, версия «unknown» скрыта.
        let local = by_name("local-tool").expect("локальный тоже показан");
        assert!(local.installed);
        assert_eq!(local.marketplace, "my-mkt");
        assert_eq!(local.version, None, "«unknown» не показываем как версию");
    }

    #[test]
    fn broken_inputs_do_not_panic() {
        assert!(parse_claude_plugins("", "", "").is_empty());
        assert!(parse_claude_plugins("{ битый", "тоже", "и это").is_empty());
        // Только установленные (каталога нет) — всё равно список.
        let only_installed = parse_claude_plugins("{}", INSTALLED, SETTINGS);
        assert_eq!(
            only_installed.len(),
            2,
            "оба установленных показаны без каталога"
        );
    }

    // Фикстура по РЕАЛЬНОМУ формату `~/.claude/plugins/known_marketplaces.json`.
    const KNOWN_MARKETPLACES: &str = r#"{
        "claude-plugins-official": {
            "source": {"source": "github", "repo": "anthropics/claude-plugins-official"},
            "installLocation": "/x"
        },
        "agricidaniel-seo": {
            "source": {"source": "git", "url": "https://github.com/AgriciDaniel/claude-seo.git"}
        }
    }"#;

    #[test]
    fn parse_marketplaces_reads_github_repo_and_git_url() {
        let markets = parse_claude_marketplaces(KNOWN_MARKETPLACES);
        let by_name = |n: &str| markets.iter().find(|m| m.name == n).cloned();

        let official = by_name("claude-plugins-official").expect("github-источник");
        assert_eq!(official.provider, Provider::Claude);
        assert_eq!(
            official.source, "anthropics/claude-plugins-official",
            "у github берём repo"
        );

        let seo = by_name("agricidaniel-seo").expect("git-источник");
        assert_eq!(
            seo.source, "https://github.com/AgriciDaniel/claude-seo.git",
            "у git берём url"
        );
    }

    #[test]
    fn parse_marketplaces_broken_input_is_empty_without_panic() {
        assert!(parse_claude_marketplaces("").is_empty());
        assert!(parse_claude_marketplaces("{ битый").is_empty());
        assert!(
            parse_claude_marketplaces("[]").is_empty(),
            "не-объект → пусто"
        );
    }

    #[test]
    fn marketplace_cmds_build_add_and_remove() {
        assert_eq!(
            cmd_args(&claude_marketplace_add_cmd("owner/repo")),
            ["plugin", "marketplace", "add", "owner/repo"]
        );
        assert_eq!(
            cmd_args(&claude_marketplace_remove_cmd("my-mkt")),
            ["plugin", "marketplace", "remove", "my-mkt"]
        );
    }

    #[test]
    fn parse_details_reads_description_and_author_from_marketplace_entry() {
        // Реальный формат: описание/автор вложены в marketplace_entry.
        let catalog = r#"{"catalog":{"plugins":{
            "42crunch@official":{"version":"1.9.0","marketplace_entry":{
                "description":"  Audit OpenAPI specs  ","author":{"name":"42Crunch","email":"x@y"}}},
            "nodesc@official":{"version":"1.0","marketplace_entry":{"author":{"name":"x"}}}
        }}}"#;
        let details = parse_claude_plugin_details(catalog);

        let d = details
            .get("42crunch@official")
            .expect("описание разобрано");
        assert_eq!(d.description, "Audit OpenAPI specs", "описание тримится");
        assert_eq!(
            d.author.as_deref(),
            Some("42Crunch"),
            "автор — из author.name"
        );
        assert!(
            !details.contains_key("nodesc@official"),
            "без описания запись пропущена — показывать нечего"
        );
    }

    #[test]
    fn parse_details_broken_input_is_empty() {
        assert!(parse_claude_plugin_details("").is_empty());
        assert!(parse_claude_plugin_details("{ битый").is_empty());
        assert!(parse_claude_plugin_details("{}").is_empty());
    }

    #[test]
    fn plugin_updates_flag_installed_versions_behind_catalog() {
        let catalog = r#"{"catalog":{"plugins":{
            "old@m":{"version":"2.0.0"},
            "fresh@m":{"version":"1.0.0"},
            "unk@m":{"version":"3.0.0"}
        }}}"#;
        let installed = r#"{"version":2,"plugins":{
            "old@m":[{"version":"1.0.0"}],
            "fresh@m":[{"version":"1.0.0"}],
            "unk@m":[{"version":"unknown"}]
        }}"#;
        let updates = parse_claude_plugin_updates(catalog, installed);

        assert!(
            updates.contains("old@m"),
            "installed 1.0.0 ≠ catalog 2.0.0 → апдейт"
        );
        assert!(!updates.contains("fresh@m"), "версии совпали → без апдейта");
        assert!(
            !updates.contains("unk@m"),
            "unknown-версия не судится (нечего сравнивать)"
        );
        assert!(
            parse_claude_plugin_updates("", "").is_empty(),
            "битый вход — пусто"
        );
    }

    #[test]
    fn installed_plugins_sort_before_available() {
        // «zebra» установлен, «alpha» только доступен. На реальном каталоге доступных — сотни,
        // и установленный обязан быть ВВЕРХУ, несмотря на алфавит (иначе тонет и недостижим).
        let catalog = r#"{"catalog":{"plugins":{
            "alpha@m":{"version":"1"},
            "zebra@m":{"version":"1"}
        }}}"#;
        let installed = r#"{"version":2,"plugins":{"zebra@m":[{"version":"1"}]}}"#;
        let plugins = parse_claude_plugins(catalog, installed, "{}");
        let names: Vec<&str> = plugins.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            ["zebra", "alpha"],
            "установленный zebra выше доступного alpha"
        );
        assert!(plugins[0].installed && !plugins[1].installed);
    }
}
