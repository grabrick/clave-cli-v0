use super::*;

#[derive(Clone, Copy)]
pub(crate) struct EffortSnapshot {
    pub(crate) effort_index: usize,
    pub(crate) codex_effort_index: usize,
    pub(crate) claude_effort_index: usize,
    pub(crate) linked_effort_split: bool,
    pub(crate) effort_focus: usize,
}

impl App {
    pub(crate) fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.effort_focus = 0;
        self.effort_index = normalize_common_effort_index(self.effort_index);
        self.codex_effort_index = normalize_provider_effort_index("codex", self.codex_effort_index);
        self.claude_effort_index =
            normalize_provider_effort_index("claude", self.claude_effort_index);
    }

    pub(crate) fn effort_summary(&self) -> String {
        match self.mode {
            Mode::CodexOnly => format!("codex {}", effort_label(self.codex_effort_index)),
            Mode::ClaudeOnly => format!("claude {}", effort_label(self.claude_effort_index)),
            Mode::ClaudeCodex | Mode::CodexClaude if self.linked_effort_split => format!(
                "claude {} · codex {}",
                effort_label(self.claude_effort_index),
                effort_label(self.codex_effort_index)
            ),
            Mode::ClaudeCodex | Mode::CodexClaude => {
                format!("shared {}", effort_label(self.effort_index))
            }
        }
    }

    /// Читаемое усилие для футера: «Claude max · Codex xhigh» при раздельном, иначе одно.
    pub(crate) fn human_effort_summary(&self) -> String {
        match self.mode {
            Mode::CodexOnly => effort_label(self.codex_effort_index).to_string(),
            Mode::ClaudeOnly => effort_label(self.claude_effort_index).to_string(),
            Mode::ClaudeCodex | Mode::CodexClaude if self.linked_effort_split => format!(
                "Claude {} · Codex {}",
                effort_label(self.claude_effort_index),
                effort_label(self.codex_effort_index)
            ),
            Mode::ClaudeCodex | Mode::CodexClaude => effort_label(self.effort_index).to_string(),
        }
    }

    pub(crate) fn provider_effort(&self, provider: &str) -> &'static str {
        if matches!(self.mode, Mode::ClaudeCodex | Mode::CodexClaude) && !self.linked_effort_split {
            return effort_label(self.effort_index);
        }

        match provider {
            "claude" => effort_label(self.claude_effort_index),
            "codex" => effort_label(self.codex_effort_index),
            _ => effort_label(self.effort_index),
        }
    }

    pub(crate) fn effort_snapshot(&self) -> EffortSnapshot {
        EffortSnapshot {
            effort_index: self.effort_index,
            codex_effort_index: self.codex_effort_index,
            claude_effort_index: self.claude_effort_index,
            linked_effort_split: self.linked_effort_split,
            effort_focus: self.effort_focus,
        }
    }

    pub(crate) fn restore_effort_snapshot(&mut self, snapshot: EffortSnapshot) {
        self.effort_index = snapshot.effort_index;
        self.codex_effort_index = snapshot.codex_effort_index;
        self.claude_effort_index = snapshot.claude_effort_index;
        self.linked_effort_split = snapshot.linked_effort_split;
        self.effort_focus = snapshot.effort_focus;
    }

    pub(crate) fn effort_picker_rows(&self) -> usize {
        match self.mode {
            Mode::ClaudeCodex | Mode::CodexClaude if self.linked_effort_split => 3,
            Mode::ClaudeCodex | Mode::CodexClaude => 2,
            _ => 1,
        }
    }

    pub(crate) fn adjust_effort_focus(&mut self, direction: isize) {
        match self.mode {
            Mode::CodexOnly => {
                self.codex_effort_index =
                    move_provider_effort_index("codex", self.codex_effort_index, direction);
            }
            Mode::ClaudeOnly => {
                self.claude_effort_index =
                    move_provider_effort_index("claude", self.claude_effort_index, direction);
            }
            Mode::ClaudeCodex | Mode::CodexClaude => match self.effort_focus {
                0 => {
                    self.linked_effort_split = !self.linked_effort_split;
                    self.effort_index = normalize_common_effort_index(self.effort_index);
                    if self.effort_focus >= self.effort_picker_rows() {
                        self.effort_focus = self.effort_picker_rows().saturating_sub(1);
                    }
                }
                1 if self.linked_effort_split => {
                    self.claude_effort_index =
                        move_provider_effort_index("claude", self.claude_effort_index, direction);
                }
                2 if self.linked_effort_split => {
                    self.codex_effort_index =
                        move_provider_effort_index("codex", self.codex_effort_index, direction);
                }
                1 => {
                    self.effort_index = move_common_effort_index(self.effort_index, direction);
                }
                _ => {}
            },
        }
    }

    pub(crate) fn adjust_startup_effort(&mut self, direction: isize) {
        match self.mode {
            Mode::CodexOnly => {
                self.codex_effort_index =
                    move_provider_effort_index("codex", self.codex_effort_index, direction);
            }
            Mode::ClaudeOnly => {
                self.claude_effort_index =
                    move_provider_effort_index("claude", self.claude_effort_index, direction);
            }
            Mode::ClaudeCodex | Mode::CodexClaude if self.linked_effort_split => {
                self.linked_effort_split = false;
                self.effort_index = normalize_common_effort_index(self.effort_index);
            }
            Mode::ClaudeCodex | Mode::CodexClaude => {
                self.effort_index = move_common_effort_index(self.effort_index, direction);
            }
        }
    }
}
