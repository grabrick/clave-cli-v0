use crate::*;

pub(crate) fn auth_requirements_ready(mode: Mode, onboarding: &Onboarding) -> bool {
    (!mode.needs_codex() || onboarding.codex_authenticated)
        && (!mode.needs_claude() || onboarding.claude_authenticated)
}

pub(crate) fn missing_auth_text(mode: Mode, onboarding: &Onboarding, lang: Language) -> String {
    let mut missing = Vec::new();
    if mode.needs_codex() && !onboarding.codex_authenticated {
        missing.push(if onboarding.codex_installed {
            "Codex"
        } else {
            lang.choose("Codex CLI не найден", "Codex CLI missing")
        });
    }
    if mode.needs_claude() && !onboarding.claude_authenticated {
        missing.push(if onboarding.claude_installed {
            "Claude"
        } else {
            lang.choose("Claude CLI не найден", "Claude CLI missing")
        });
    }

    if missing.is_empty() {
        lang.choose("всё готово", "all ready").to_string()
    } else {
        missing.join(" + ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn onboarding(
        codex_authenticated: bool,
        claude_authenticated: bool,
        codex_installed: bool,
        claude_installed: bool,
    ) -> Onboarding {
        Onboarding {
            step: OnboardingStep::Auth,
            provider_index: 0,
            setting_index: 0,
            codex_installed,
            claude_installed,
            codex_authenticated,
            claude_authenticated,
            codex_status: String::new(),
            claude_status: String::new(),
            message: String::new(),
        }
    }

    #[test]
    fn auth_requirements_ready_checks_only_needed_providers() {
        // Нужен только Codex: решает его логин, состояние Claude не важно.
        assert!(auth_requirements_ready(
            Mode::CodexOnly,
            &onboarding(true, false, true, true)
        ));
        assert!(!auth_requirements_ready(
            Mode::CodexOnly,
            &onboarding(false, true, true, true)
        ));

        // Нужен только Claude: зеркально.
        assert!(auth_requirements_ready(
            Mode::ClaudeOnly,
            &onboarding(false, true, true, true)
        ));
        assert!(!auth_requirements_ready(
            Mode::ClaudeOnly,
            &onboarding(true, false, true, true)
        ));

        // Нужны оба: половины логина не хватает.
        assert!(!auth_requirements_ready(
            Mode::ClaudeCodex,
            &onboarding(true, false, true, true)
        ));
        assert!(!auth_requirements_ready(
            Mode::ClaudeCodex,
            &onboarding(false, true, true, true)
        ));
        assert!(!auth_requirements_ready(
            Mode::ClaudeCodex,
            &onboarding(false, false, true, true)
        ));
        assert!(auth_requirements_ready(
            Mode::ClaudeCodex,
            &onboarding(true, true, true, true)
        ));
        assert!(!auth_requirements_ready(
            Mode::CodexClaude,
            &onboarding(true, false, true, true)
        ));
        assert!(auth_requirements_ready(
            Mode::CodexClaude,
            &onboarding(true, true, true, true)
        ));
    }

    #[test]
    fn missing_auth_text_lists_only_needed_and_unauthenticated() {
        // Всё нужное залогинено — сообщения о нехватке нет.
        assert_eq!(
            missing_auth_text(
                Mode::CodexOnly,
                &onboarding(true, false, true, true),
                Language::Ru
            ),
            "всё готово"
        );
        assert_eq!(
            missing_auth_text(
                Mode::CodexOnly,
                &onboarding(true, false, true, true),
                Language::En
            ),
            "all ready"
        );

        // Нужен Codex и не залогинен, Claude не нужен и тоже не залогинен — только Codex.
        assert_eq!(
            missing_auth_text(
                Mode::CodexOnly,
                &onboarding(false, false, true, true),
                Language::Ru
            ),
            "Codex"
        );
        // Зеркально: нужен только Claude.
        assert_eq!(
            missing_auth_text(
                Mode::ClaudeOnly,
                &onboarding(false, false, true, true),
                Language::Ru
            ),
            "Claude"
        );
        assert_eq!(
            missing_auth_text(
                Mode::ClaudeOnly,
                &onboarding(false, true, true, true),
                Language::Ru
            ),
            "всё готово"
        );

        // Оба нужны, оба не залогинены и не установлены — обе локализованные ветки и разделитель.
        assert_eq!(
            missing_auth_text(
                Mode::ClaudeCodex,
                &onboarding(false, false, false, false),
                Language::Ru
            ),
            "Codex CLI не найден + Claude CLI не найден"
        );
        assert_eq!(
            missing_auth_text(
                Mode::ClaudeCodex,
                &onboarding(false, false, false, false),
                Language::En
            ),
            "Codex CLI missing + Claude CLI missing"
        );
    }
}
