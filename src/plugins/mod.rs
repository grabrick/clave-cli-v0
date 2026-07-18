//! Бэкенды плагинов: чистая логика без спавна и без чтения файлов. Разбирают уже полученный
//! вывод CLI/конфиг в унифицированные [`PluginEntry`] и (в следующих фазах) строят команды
//! действий. Спавн и I/O — в слое `app`/`worker`; сюда данные приходят строкой. За счёт этого
//! бэкенды тестируются фикстурами, не трогая реальные `~/.claude`/`~/.codex` (урок BUG-006).

pub(crate) mod claude;
pub(crate) mod codex;

pub(crate) use claude::{
    claude_action_cmd, claude_marketplace_add_cmd, claude_marketplace_remove_cmd,
    parse_claude_marketplaces, parse_claude_plugin_details, parse_claude_plugin_updates,
    parse_claude_plugins,
};
pub(crate) use codex::{
    codex_action_cmd, codex_marketplace_add_cmd, codex_marketplace_remove_cmd,
    parse_codex_marketplaces, parse_codex_plugins,
};

/// Действие над плагином. Команду строит бэкенд провайдера — claude и codex делают это
/// по-разному (например, вкл/выкл: claude — `plugin enable`, codex — через features).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PluginAction {
    Install,
    Uninstall,
    Enable,
    Disable,
    Update,
}

/// Упорядочивает записи провайдера для панели: установленные — ВВЕРХ (на реальном каталоге
/// доступных сотни, и установленные иначе тонут и недостижимы), внутри — по имени. Вызывается
/// в конце каждого бэкенд-парсера, чтобы порядок был детерминирован и тестируем.
pub(crate) fn sort_plugins(plugins: &mut [crate::PluginEntry]) {
    plugins.sort_by(|a, b| (!a.installed, &a.name).cmp(&(!b.installed, &b.name)));
}
