use super::Language;
use crate::prelude::*;

/// Режим прямого чата, переключается по Shift+Tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ChatMode {
    #[default]
    Discussion,
    Plan,
    FullAccess,
    Tandem,
}

impl ChatMode {
    pub(crate) fn next(self) -> Self {
        match self {
            ChatMode::Discussion => ChatMode::Plan,
            ChatMode::Plan => ChatMode::FullAccess,
            ChatMode::FullAccess => ChatMode::Tandem,
            ChatMode::Tandem => ChatMode::Discussion,
        }
    }

    pub(crate) fn label(self, lang: Language) -> &'static str {
        match self {
            ChatMode::Discussion => lang.choose(">> Обсуждение", ">> Discussion"),
            ChatMode::Plan => lang.choose(">> Режим плана", ">> Plan Mode"),
            ChatMode::FullAccess => lang.choose(">> Полный доступ", ">> Full Access"),
            ChatMode::Tandem => lang.choose(">> Тандем", ">> Tandem"),
        }
    }

    pub(crate) fn color(self) -> Color {
        match self {
            ChatMode::Discussion => Color::Gray,
            ChatMode::Plan => Color::Indexed(80),        // cyan
            ChatMode::FullAccess => Color::Indexed(120), // green
            ChatMode::Tandem => Color::Indexed(170),     // magenta
        }
    }

    /// Инструменты claude (`--tools`). Пусто = чистый чат. Plan правит файлы, но без Bash.
    pub(crate) fn claude_tools(self) -> &'static str {
        match self {
            ChatMode::Discussion => "",
            ChatMode::Plan => "Read Edit Write Grep Glob",
            ChatMode::FullAccess => "Read Edit Write Bash Grep Glob",
            ChatMode::Tandem => "",
        }
    }

    pub(crate) fn claude_permission(self) -> &'static str {
        match self {
            ChatMode::Discussion => "default",
            ChatMode::Plan => "acceptEdits",
            ChatMode::FullAccess => "bypassPermissions",
            ChatMode::Tandem => "default",
        }
    }

    pub(crate) fn codex_sandbox(self) -> &'static str {
        match self {
            ChatMode::Discussion => "read-only",
            ChatMode::Plan | ChatMode::FullAccess => "workspace-write",
            ChatMode::Tandem => "read-only",
        }
    }

    /// Режим не меняет файлы и не запускает команды (нет Edit/Write/Bash, песочница
    /// read-only). Plan и FullAccess правят рабочую директорию, поэтому не read-only.
    pub(crate) fn is_read_only(self) -> bool {
        matches!(self, ChatMode::Discussion | ChatMode::Tandem)
    }

    pub(crate) fn prompt_hint(self, lang: Language) -> &'static str {
        match self {
            ChatMode::Discussion => lang.choose(
                "Просто ответь на сообщение. Не используй инструменты и не меняй файлы.",
                "Just answer the message. Do not use tools or modify files.",
            ),
            ChatMode::Plan => lang.choose(
                "Составь план и реализуй его: можешь читать и править файлы, но не выполняй произвольные команды (Bash).",
                "Make a plan and carry it out: you may read and edit files, but do not run arbitrary shell commands (Bash).",
            ),
            ChatMode::FullAccess => lang.choose(
                "Ты автономный агент: читай, создавай и правь файлы и выполняй команды в рабочей директории — решай всё сам.",
                "You are an autonomous agent: read, create and edit files and run commands in the working directory — decide everything yourself.",
            ),
            ChatMode::Tandem => lang.choose(
                "Тандемный режим: исполнитель и критик работают в паре.",
                "Tandem mode: an executor and a critic work as a pair.",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_mode_cycles_and_maps_flags() {
        assert_eq!(ChatMode::default(), ChatMode::Discussion);
        assert_eq!(ChatMode::Discussion.next(), ChatMode::Plan);
        assert_eq!(ChatMode::Plan.next(), ChatMode::FullAccess);
        assert_eq!(ChatMode::FullAccess.next(), ChatMode::Tandem);
        assert_eq!(ChatMode::Tandem.next(), ChatMode::Discussion);

        // Discussion — чистый чат
        assert_eq!(ChatMode::Discussion.claude_tools(), "");
        assert_eq!(ChatMode::Discussion.codex_sandbox(), "read-only");

        // Plan правит файлы, но без Bash
        assert!(ChatMode::Plan.claude_tools().contains("Edit"));
        assert!(!ChatMode::Plan.claude_tools().contains("Bash"));
        assert_eq!(ChatMode::Plan.codex_sandbox(), "workspace-write");

        // Full — всё, включая Bash
        assert!(ChatMode::FullAccess.claude_tools().contains("Bash"));
        assert_eq!(
            ChatMode::FullAccess.claude_permission(),
            "bypassPermissions"
        );

        // Tandem существует в цикле и имеет метку
        assert!(ChatMode::Tandem.label(Language::En).contains("Tandem"));

        // Read-only: Discussion и Tandem не правят ФС; Plan и FullAccess — правят.
        assert!(ChatMode::Discussion.is_read_only());
        assert!(ChatMode::Tandem.is_read_only());
        assert!(!ChatMode::Plan.is_read_only());
        assert!(!ChatMode::FullAccess.is_read_only());
    }
}
