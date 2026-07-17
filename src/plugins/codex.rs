use crate::prelude::*;
use crate::*;

/// Разбирает вывод `codex plugin list --available --json` в унифицированные записи. Формат:
/// `{ "installed": [...], "available": [...] }`, где запись несёт `name`, `marketplaceName`,
/// `version`, `installed`, `enabled`. Битый/пустой JSON → пустой список (панель покажет
/// «не удалось загрузить», без паники — как lossy-декодирование в worker).
pub(crate) fn parse_codex_plugins(json: &str) -> Vec<PluginEntry> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for section in ["installed", "available"] {
        let Some(items) = value.get(section).and_then(|v| v.as_array()) else {
            continue;
        };
        out.extend(items.iter().filter_map(codex_entry));
    }
    out
}

fn codex_entry(item: &serde_json::Value) -> Option<PluginEntry> {
    let name = item.get("name")?.as_str()?.to_string();
    Some(PluginEntry {
        provider: Provider::Codex,
        name,
        marketplace: item
            .get("marketplaceName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        installed: item
            .get("installed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        enabled: item
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        version: item
            .get("version")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

/// Строит команду действия codex. Установка/обновление — `plugin add`, удаление — `plugin
/// remove`, вкл/выкл — через features (`plugin list --enable/--disable <name>`, эквивалент
/// `-c features.<name>=bool`; отдельной команды `update`/`enable` у codex нет).
pub(crate) fn codex_action_cmd(action: PluginAction, entry: &PluginEntry) -> Command {
    let mut cmd = Command::new(codex_binary());
    match action {
        PluginAction::Install | PluginAction::Update => {
            cmd.args(["plugin", "add", &entry.qualified_name()]);
        }
        PluginAction::Uninstall => {
            cmd.args(["plugin", "remove", &entry.qualified_name()]);
        }
        PluginAction::Enable => {
            cmd.args(["plugin", "list", "--enable", &entry.name]);
        }
        PluginAction::Disable => {
            cmd.args(["plugin", "list", "--disable", &entry.name]);
        }
    }
    cmd
}

/// Разбирает `codex plugin marketplace list --json` в список источников. Формат:
/// `{ "marketplaces": [ { "name", "root", "marketplaceSource": { "source" } } ] }`. Адрес берём
/// из `marketplaceSource.source`, а если его нет (встроенные источники несут только `root`) —
/// из `root`. Битый/пустой JSON → пусто, без паники.
pub(crate) fn parse_codex_marketplaces(json: &str) -> Vec<Marketplace> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(items) = value.get("marketplaces").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    items.iter().filter_map(codex_marketplace_entry).collect()
}

fn codex_marketplace_entry(item: &serde_json::Value) -> Option<Marketplace> {
    let name = item.get("name")?.as_str()?.to_string();
    let source = item
        .get("marketplaceSource")
        .and_then(|s| s.get("source"))
        .and_then(|v| v.as_str())
        .or_else(|| item.get("root").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    Some(Marketplace {
        provider: Provider::Codex,
        name,
        source,
    })
}

/// `codex plugin marketplace add <источник>` — локальный путь или Git-репозиторий.
pub(crate) fn codex_marketplace_add_cmd(source: &str) -> Command {
    let mut cmd = Command::new(codex_binary());
    cmd.args(["plugin", "marketplace", "add", source]);
    cmd
}

/// `codex plugin marketplace remove <имя>` — удаление источника по имени.
pub(crate) fn codex_marketplace_remove_cmd(name: &str) -> Command {
    let mut cmd = Command::new(codex_binary());
    cmd.args(["plugin", "marketplace", "remove", name]);
    cmd
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
    fn action_cmd_maps_to_codex_verbs() {
        let entry = PluginEntry {
            provider: Provider::Codex,
            name: "documents".into(),
            marketplace: "openai".into(),
            installed: true,
            enabled: true,
            version: None,
        };
        assert_eq!(
            cmd_args(&codex_action_cmd(PluginAction::Install, &entry)),
            ["plugin", "add", "documents@openai"]
        );
        assert_eq!(
            cmd_args(&codex_action_cmd(PluginAction::Uninstall, &entry)),
            ["plugin", "remove", "documents@openai"]
        );
        assert_eq!(
            cmd_args(&codex_action_cmd(PluginAction::Disable, &entry)),
            ["plugin", "list", "--disable", "documents"]
        );
    }

    // Фикстура по РЕАЛЬНОМУ формату `codex plugin list --available --json`.
    const REAL: &str = r#"{
      "installed": [
        {"pluginId":"documents@openai-primary-runtime","name":"documents",
         "marketplaceName":"openai-primary-runtime","version":"26.709.11516",
         "installed":true,"enabled":true}
      ],
      "available": [
        {"pluginId":"chrome@openai-bundled","name":"chrome",
         "marketplaceName":"openai-bundled","version":"26.707.71524",
         "installed":false,"enabled":false}
      ]
    }"#;

    #[test]
    fn parses_installed_and_available_sections() {
        let plugins = parse_codex_plugins(REAL);
        assert_eq!(plugins.len(), 2, "обе секции разобраны");

        let installed = &plugins[0];
        assert_eq!(installed.name, "documents");
        assert_eq!(installed.marketplace, "openai-primary-runtime");
        assert!(installed.installed && installed.enabled);
        assert_eq!(installed.version.as_deref(), Some("26.709.11516"));
        assert_eq!(installed.provider, Provider::Codex);

        let available = &plugins[1];
        assert_eq!(available.name, "chrome");
        assert!(
            !available.installed && !available.enabled,
            "доступный, не установлен"
        );
    }

    #[test]
    fn broken_or_empty_json_yields_no_plugins_without_panic() {
        assert!(parse_codex_plugins("").is_empty());
        assert!(parse_codex_plugins("{ не json").is_empty());
        assert!(parse_codex_plugins("{}").is_empty(), "нет секций — пусто");
        // Запись без имени пропускается, а не роняет разбор.
        assert!(parse_codex_plugins(r#"{"installed":[{"version":"1"}]}"#).is_empty());
    }

    // Фикстура по РЕАЛЬНОМУ формату `codex plugin marketplace list --json`. Третий источник —
    // без `marketplaceSource` (только `root`): адрес должен взяться из `root`.
    const MARKETPLACES: &str = r#"{"marketplaces":[
        {"name":"openai-primary-runtime","root":"/a/primary",
         "marketplaceSource":{"sourceType":"local","source":"/a/primary"}},
        {"name":"openai-curated","root":"/b/curated"}
    ]}"#;

    #[test]
    fn parse_marketplaces_uses_source_then_falls_back_to_root() {
        let markets = parse_codex_marketplaces(MARKETPLACES);
        assert_eq!(markets.len(), 2);

        assert_eq!(markets[0].name, "openai-primary-runtime");
        assert_eq!(markets[0].provider, Provider::Codex);
        assert_eq!(
            markets[0].source, "/a/primary",
            "адрес из marketplaceSource"
        );

        assert_eq!(
            markets[1].source, "/b/curated",
            "без marketplaceSource берём root"
        );
    }

    #[test]
    fn parse_marketplaces_broken_input_is_empty_without_panic() {
        assert!(parse_codex_marketplaces("").is_empty());
        assert!(parse_codex_marketplaces("{ битый").is_empty());
        assert!(
            parse_codex_marketplaces("{}").is_empty(),
            "нет ключа — пусто"
        );
        // Запись без имени пропускается, не роняя разбор.
        assert!(parse_codex_marketplaces(r#"{"marketplaces":[{"root":"/x"}]}"#).is_empty());
    }

    #[test]
    fn marketplace_cmds_build_add_and_remove() {
        assert_eq!(
            cmd_args(&codex_marketplace_add_cmd("/local/path")),
            ["plugin", "marketplace", "add", "/local/path"]
        );
        assert_eq!(
            cmd_args(&codex_marketplace_remove_cmd("openai-curated")),
            ["plugin", "marketplace", "remove", "openai-curated"]
        );
    }
}
