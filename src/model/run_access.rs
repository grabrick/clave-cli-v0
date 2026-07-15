use super::ChatMode;

/// Права запуска CLI для одного вызова. `ChatMode` отвечает за UI (label/цвет),
/// `RunAccess` — за то, какие инструменты/песочница уходят провайдеру. Фазам
/// плана нужны наборы, которых нет среди пользовательских режимов.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunAccess {
    /// Обычный чат: права берутся из текущего режима.
    Chat(ChatMode),
    /// Фаза 1 плана: только чтение кода, без правок и команд.
    PlanReadonly,
    /// Фаза 2 плана: полный доступ для выполнения одобренного плана.
    PlanExecute,
}

impl RunAccess {
    pub(crate) fn claude_tools(self) -> &'static str {
        match self {
            RunAccess::Chat(mode) => mode.claude_tools(),
            RunAccess::PlanReadonly => "Read Grep Glob",
            RunAccess::PlanExecute => "Read Edit Write Bash Grep Glob",
        }
    }

    pub(crate) fn claude_permission(self) -> &'static str {
        match self {
            RunAccess::Chat(mode) => mode.claude_permission(),
            RunAccess::PlanReadonly => "default",
            RunAccess::PlanExecute => "bypassPermissions",
        }
    }

    pub(crate) fn codex_sandbox(self) -> &'static str {
        match self {
            RunAccess::Chat(mode) => mode.codex_sandbox(),
            RunAccess::PlanReadonly => "read-only",
            RunAccess::PlanExecute => "workspace-write",
        }
    }

    /// Ран НЕ трогает рабочую директорию: без Edit/Write/Bash, песочница read-only.
    /// Только такой ран безопасно повторять автоматически при транзиентном сбое —
    /// мутирующий ран при повторе применил бы необратимые побочные эффекты (правки
    /// файлов, Bash, git-коммит) второй раз.
    pub(crate) fn is_read_only(self) -> bool {
        match self {
            RunAccess::PlanReadonly => true,
            RunAccess::PlanExecute => false,
            RunAccess::Chat(mode) => mode.is_read_only(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_access_maps_phase_flags() {
        // Фаза 1 — чтение без правок и без Bash.
        assert!(RunAccess::PlanReadonly.claude_tools().contains("Read"));
        assert!(!RunAccess::PlanReadonly.claude_tools().contains("Edit"));
        assert!(!RunAccess::PlanReadonly.claude_tools().contains("Bash"));
        assert_eq!(RunAccess::PlanReadonly.codex_sandbox(), "read-only");

        // Фаза 2 — полный доступ.
        assert!(RunAccess::PlanExecute.claude_tools().contains("Bash"));
        assert_eq!(
            RunAccess::PlanExecute.claude_permission(),
            "bypassPermissions"
        );
        assert_eq!(RunAccess::PlanExecute.codex_sandbox(), "workspace-write");

        // Chat делегирует режиму.
        assert_eq!(RunAccess::Chat(ChatMode::Discussion).claude_tools(), "");
        assert_eq!(
            RunAccess::Chat(ChatMode::FullAccess).codex_sandbox(),
            "workspace-write"
        );

        // Read-only = безопасно повторять при транзиентном сбое (не трогает ФС).
        assert!(RunAccess::PlanReadonly.is_read_only());
        assert!(!RunAccess::PlanExecute.is_read_only());
        assert!(RunAccess::Chat(ChatMode::Discussion).is_read_only());
        assert!(!RunAccess::Chat(ChatMode::Plan).is_read_only());
        assert!(!RunAccess::Chat(ChatMode::FullAccess).is_read_only());
    }
}
