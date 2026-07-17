use crate::Provider;

/// Один плагин провайдера в панели `/plugins`: установленный или доступный из marketplace.
/// Общая модель для Claude и Codex — секции панели фильтруют по `provider`, а различия
/// команд прячутся в бэкендах (`plugins::ClaudePlugins` / `plugins::CodexPlugins`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PluginEntry {
    pub(crate) provider: Provider,
    pub(crate) name: String,
    /// Marketplace-источник: из `context7@claude-plugins-official` это `claude-plugins-official`.
    /// Пусто, если провайдер не сообщает источник (тогда действия идут по одному имени).
    pub(crate) marketplace: String,
    pub(crate) installed: bool,
    pub(crate) enabled: bool,
    pub(crate) version: Option<String>,
}

impl PluginEntry {
    /// Полное имя `plugin@marketplace` — то, что принимают команды install/add. Без источника
    /// (пустой `marketplace`) — просто имя.
    pub(crate) fn qualified_name(&self) -> String {
        if self.marketplace.is_empty() {
            self.name.clone()
        } else {
            format!("{}@{}", self.name, self.marketplace)
        }
    }
}

/// Marketplace-источник плагинов провайдера (панель `/plugins`, режим источников). Claude берёт
/// их из `~/.claude/plugins/known_marketplaces.json`, Codex — из `plugin marketplace list --json`.
/// `source` — человекочитаемый адрес (GitHub `owner/repo`, git-URL или локальный путь), из которого
/// источник добавлен; `name` — его короткое имя, по которому источник удаляют.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Marketplace {
    pub(crate) provider: Provider,
    pub(crate) name: String,
    pub(crate) source: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualified_name_joins_plugin_and_marketplace() {
        let entry = PluginEntry {
            provider: Provider::Claude,
            name: "context7".to_string(),
            marketplace: "claude-plugins-official".to_string(),
            installed: true,
            enabled: true,
            version: Some("1.0".to_string()),
        };
        assert_eq!(entry.qualified_name(), "context7@claude-plugins-official");
    }

    #[test]
    fn qualified_name_without_marketplace_is_bare() {
        let entry = PluginEntry {
            provider: Provider::Codex,
            name: "sites".to_string(),
            marketplace: String::new(),
            installed: false,
            enabled: false,
            version: None,
        };
        assert_eq!(entry.qualified_name(), "sites");
    }
}
