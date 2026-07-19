use super::*;

pub(crate) fn provider_count() -> usize {
    4
}

pub(crate) fn provider_mode(index: usize) -> Mode {
    match index {
        0 => Mode::CodexOnly,
        1 => Mode::ClaudeCodex,
        2 => Mode::CodexClaude,
        3 => Mode::ClaudeOnly,
        _ => Mode::CodexOnly,
    }
}

pub(crate) fn provider_index(mode: Mode) -> usize {
    match mode {
        Mode::CodexOnly => 0,
        Mode::ClaudeCodex => 1,
        Mode::CodexClaude => 2,
        Mode::ClaudeOnly => 3,
    }
}

pub(crate) fn provider_description(mode: Mode, lang: Language) -> &'static str {
    match mode {
        Mode::CodexOnly => lang.choose("Codex пишет и ревьюит", "Codex drafts and reviews"),
        Mode::ClaudeCodex => lang.choose(
            "Claude пишет, Codex ревьюит",
            "Claude drafts, Codex reviews",
        ),
        Mode::CodexClaude => lang.choose(
            "Codex пишет, Claude ревьюит",
            "Codex drafts, Claude reviews",
        ),
        Mode::ClaudeOnly => lang.choose("Claude пишет и ревьюит", "Claude drafts and reviews"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Каждый режим — свой слот. Четыре разных числа валят `->0` и `->1` (67:5).
    #[test]
    fn provider_index_maps_each_mode_to_its_own_slot() {
        assert_eq!(provider_index(Mode::CodexOnly), 0);
        assert_eq!(provider_index(Mode::ClaudeCodex), 1);
        assert_eq!(provider_index(Mode::CodexClaude), 2);
        assert_eq!(provider_index(Mode::ClaudeOnly), 3);
    }

    /// Обратное отображение + круговой проход. `provider_mode(2)` == `CodexClaude`
    /// (и round-trip) валит delete match arm 2 (60): без арма 2 индекс 2 съехал бы
    /// в дефолт `CodexOnly` и round-trip бы сломался.
    #[test]
    fn provider_mode_is_the_inverse_of_provider_index() {
        assert_eq!(provider_mode(0), Mode::CodexOnly);
        assert_eq!(provider_mode(1), Mode::ClaudeCodex);
        assert_eq!(provider_mode(2), Mode::CodexClaude);
        assert_eq!(provider_mode(3), Mode::ClaudeOnly);
        // Вне диапазона — откат к первому режиму.
        assert_eq!(provider_mode(4), Mode::CodexOnly);
        assert_eq!(provider_mode(usize::MAX), Mode::CodexOnly);
        for index in 0..provider_count() {
            assert_eq!(provider_index(provider_mode(index)), index);
        }
    }

    /// У каждого режима своё непустое описание на обоих языках. Непустота валит `->""`,
    /// различность — `->"xyzzy"` (76:5, при нём все четыре стали бы одинаковы).
    #[test]
    fn provider_description_is_non_empty_and_distinct_per_mode() {
        let modes = [
            Mode::CodexOnly,
            Mode::ClaudeCodex,
            Mode::CodexClaude,
            Mode::ClaudeOnly,
        ];
        for lang in [Language::Ru, Language::En] {
            let texts: Vec<&str> = modes
                .iter()
                .map(|&mode| provider_description(mode, lang))
                .collect();
            for text in &texts {
                assert!(!text.is_empty(), "описание режима пустое на {lang:?}");
            }
            let unique: std::collections::HashSet<&str> = texts.iter().copied().collect();
            assert_eq!(
                unique.len(),
                modes.len(),
                "описания режимов обязаны различаться ({lang:?})"
            );
        }
        // Точные строки, чтобы «xyzzy» и перепутанные режимы не прошли.
        assert_eq!(
            provider_description(Mode::CodexOnly, Language::Ru),
            "Codex пишет и ревьюит"
        );
        assert_eq!(
            provider_description(Mode::ClaudeOnly, Language::En),
            "Claude drafts and reviews"
        );
    }

    // ── composer_height: (строки ввода + 2), зажатое в [3, 10] ────────────────
}
