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

#[cfg(test)]
mod tests {
    use super::*;

    fn effort_app() -> App {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = std::env::temp_dir().join(format!(
            "clave-app-effort-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::create_dir_all(&dir);
        let config = AppConfig {
            onboarding_done: true,
            ..AppConfig::default()
        };
        App::from_config(
            config,
            dir.join("config.json"),
            dir.join("history"),
            dir.clone(),
        )
    }

    // ── provider_effort: усилие, которое РЕАЛЬНО уходит модели ───────────────

    /// Тандем без раздельного: оба провайдера получают ОБЩЕЕ усилие effort_index.
    #[test]
    fn provider_effort_tandem_shared_when_linked() {
        let mut app = effort_app();
        app.mode = Mode::ClaudeCodex;
        app.linked_effort_split = false;
        app.effort_index = 0; // low
        app.claude_effort_index = 4; // max
        app.codex_effort_index = 3; // xhigh
        assert_eq!(app.provider_effort("claude"), "low");
        assert_eq!(app.provider_effort("codex"), "low");
        assert_eq!(app.provider_effort("claude"), app.provider_effort("codex"));
    }

    /// Тандем + раздельно: каждый провайдер берёт СВОЙ индекс.
    #[test]
    fn provider_effort_tandem_split_is_per_provider() {
        let mut app = effort_app();
        app.mode = Mode::CodexClaude;
        app.linked_effort_split = true;
        app.effort_index = 0; // low
        app.claude_effort_index = 4; // max
        app.codex_effort_index = 3; // xhigh
        assert_eq!(app.provider_effort("claude"), "max");
        assert_eq!(app.provider_effort("codex"), "xhigh");
    }

    /// Одиночный режим: усилие из индекса своего провайдера, не из общего.
    #[test]
    fn provider_effort_single_mode_uses_provider_index() {
        let mut app = effort_app();
        app.mode = Mode::ClaudeOnly;
        app.linked_effort_split = false;
        app.effort_index = 0; // low
        app.claude_effort_index = 4; // max
        assert_eq!(app.provider_effort("claude"), "max");
    }

    // ── effort_summary / human_effort_summary ────────────────────────────────

    #[test]
    fn effort_summary_covers_all_modes() {
        let mut app = effort_app();
        app.effort_index = 0; // low
        app.claude_effort_index = 4; // max
        app.codex_effort_index = 3; // xhigh

        app.mode = Mode::CodexOnly;
        assert_eq!(app.effort_summary(), "codex xhigh");

        app.mode = Mode::ClaudeOnly;
        assert_eq!(app.effort_summary(), "claude max");

        app.mode = Mode::ClaudeCodex;
        app.linked_effort_split = true;
        assert_eq!(app.effort_summary(), "claude max · codex xhigh");

        app.linked_effort_split = false;
        assert_eq!(app.effort_summary(), "shared low");
    }

    #[test]
    fn human_effort_summary_shared_when_not_split() {
        let mut app = effort_app();
        app.mode = Mode::CodexClaude;
        app.effort_index = 0; // low
        app.claude_effort_index = 4; // max
        app.codex_effort_index = 3; // xhigh

        app.linked_effort_split = false;
        assert_eq!(app.human_effort_summary(), "low");

        app.linked_effort_split = true;
        assert_eq!(app.human_effort_summary(), "Claude max · Codex xhigh");
    }

    // ── effort_picker_rows ───────────────────────────────────────────────────

    #[test]
    fn effort_picker_rows_by_mode_and_split() {
        let mut app = effort_app();
        app.mode = Mode::ClaudeCodex;
        app.linked_effort_split = true;
        assert_eq!(app.effort_picker_rows(), 3);
        app.linked_effort_split = false;
        assert_eq!(app.effort_picker_rows(), 2);
        app.mode = Mode::CodexOnly;
        assert_eq!(app.effort_picker_rows(), 1);
    }

    // ── adjust_effort_focus: навигация ←/→ на /effort ────────────────────────

    /// Одиночный CodexOnly двигает ТОЛЬКО codex_effort_index.
    #[test]
    fn adjust_effort_focus_codex_only_moves_codex() {
        let mut app = effort_app();
        app.mode = Mode::CodexOnly;
        app.codex_effort_index = 2; // high
        app.claude_effort_index = 2; // high
        app.adjust_effort_focus(1);
        assert_eq!(app.codex_effort_index, 3, "codex high→xhigh");
        assert_eq!(app.claude_effort_index, 2, "claude не тронут");
    }

    /// Одиночный ClaudeOnly двигает ТОЛЬКО claude_effort_index.
    #[test]
    fn adjust_effort_focus_claude_only_moves_claude() {
        let mut app = effort_app();
        app.mode = Mode::ClaudeOnly;
        app.codex_effort_index = 2; // high
        app.claude_effort_index = 2; // high
        app.adjust_effort_focus(1);
        assert_eq!(app.claude_effort_index, 4, "claude high→max");
        assert_eq!(app.codex_effort_index, 2, "codex не тронут");
    }

    /// Фокус 0 в тандеме ПЕРЕКЛЮЧАЕТ split (в обе стороны) и не сдвигает сам фокус.
    #[test]
    fn adjust_effort_focus_toggles_split_on_row_zero() {
        let mut app = effort_app();
        app.mode = Mode::ClaudeCodex;
        app.effort_focus = 0;

        app.linked_effort_split = true;
        app.adjust_effort_focus(1);
        assert!(!app.linked_effort_split, "фокус 0: split true→false");
        assert_eq!(
            app.effort_focus, 0,
            "фокус остаётся 0, кламп не срабатывает"
        );

        app.adjust_effort_focus(1);
        assert!(app.linked_effort_split, "фокус 0: split false→true");
    }

    /// Фокус 1 при split двигает ТОЛЬКО claude.
    #[test]
    fn adjust_effort_focus_row_one_split_moves_claude_only() {
        let mut app = effort_app();
        app.mode = Mode::ClaudeCodex;
        app.linked_effort_split = true;
        app.effort_focus = 1;
        app.effort_index = 1; // medium
        app.claude_effort_index = 2; // high
        app.codex_effort_index = 2; // high
        app.adjust_effort_focus(1);
        assert_eq!(app.claude_effort_index, 4, "claude high→max");
        assert_eq!(app.codex_effort_index, 2, "codex не тронут");
        assert_eq!(app.effort_index, 1, "общий effort не тронут");
    }

    /// Фокус 1 без split двигает ТОЛЬКО общий effort.
    #[test]
    fn adjust_effort_focus_row_one_shared_moves_common_only() {
        let mut app = effort_app();
        app.mode = Mode::ClaudeCodex;
        app.linked_effort_split = false;
        app.effort_focus = 1;
        app.effort_index = 1; // medium
        app.claude_effort_index = 2; // high
        app.codex_effort_index = 2; // high
        app.adjust_effort_focus(1);
        assert_eq!(app.effort_index, 2, "общий medium→high");
        assert_eq!(app.claude_effort_index, 2, "claude не тронут");
        assert_eq!(app.codex_effort_index, 2, "codex не тронут");
    }

    /// Фокус 2 при split двигает ТОЛЬКО codex.
    #[test]
    fn adjust_effort_focus_row_two_split_moves_codex_only() {
        let mut app = effort_app();
        app.mode = Mode::CodexClaude;
        app.linked_effort_split = true;
        app.effort_focus = 2;
        app.effort_index = 1; // medium
        app.claude_effort_index = 2; // high
        app.codex_effort_index = 2; // high
        app.adjust_effort_focus(1);
        assert_eq!(app.codex_effort_index, 3, "codex high→xhigh");
        assert_eq!(app.claude_effort_index, 2, "claude не тронут");
        assert_eq!(app.effort_index, 1, "общий effort не тронут");
    }

    /// Фокус 2 без split — строка вне диапазона, ничего не двигается.
    #[test]
    fn adjust_effort_focus_row_two_shared_is_noop() {
        let mut app = effort_app();
        app.mode = Mode::ClaudeCodex;
        app.linked_effort_split = false;
        app.effort_focus = 2;
        app.effort_index = 1;
        app.claude_effort_index = 2;
        app.codex_effort_index = 2;
        app.adjust_effort_focus(1);
        assert_eq!(app.codex_effort_index, 2, "codex не тронут");
        assert_eq!(app.claude_effort_index, 2, "claude не тронут");
        assert_eq!(app.effort_index, 1, "общий effort не тронут");
    }

    // ── adjust_startup_effort: экран старта тандема ──────────────────────────

    /// split → сброс в shared, effort только нормализуется (не двигается).
    #[test]
    fn adjust_startup_effort_split_resets_to_shared() {
        let mut app = effort_app();
        app.mode = Mode::ClaudeCodex;
        app.linked_effort_split = true;
        app.effort_index = 1; // medium — валиден в COMMON, нормализация не сдвинет
        app.adjust_startup_effort(1);
        assert!(!app.linked_effort_split, "split сброшен в shared");
        assert_eq!(app.effort_index, 1, "усилие нормализовано, но не сдвинуто");
    }

    /// без split → двигает общий effort.
    #[test]
    fn adjust_startup_effort_shared_moves_common() {
        let mut app = effort_app();
        app.mode = Mode::CodexClaude;
        app.linked_effort_split = false;
        app.effort_index = 1; // medium
        app.adjust_startup_effort(1);
        assert_eq!(app.effort_index, 2, "общий medium→high");
        assert!(!app.linked_effort_split);
    }
}
