use crate::prelude::*;
use crate::*;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
