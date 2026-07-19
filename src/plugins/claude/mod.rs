mod catalog;
mod marketplace;
pub(crate) use catalog::*;
pub(crate) use marketplace::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::Command;
    use crate::{PluginAction, PluginEntry, Provider};

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
}
