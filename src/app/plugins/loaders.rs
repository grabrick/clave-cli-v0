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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::plugins::testkit::*;

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
}
