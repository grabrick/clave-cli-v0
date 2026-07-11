use super::constants::{CLAUDE_EFFORTS, CODEX_EFFORTS, COMMON_EFFORTS, EFFORTS};

pub(crate) fn effort_label(index: usize) -> &'static str {
    EFFORTS.get(index).copied().unwrap_or("high")
}

pub(crate) fn provider_supports_effort(provider: &str, effort: &str) -> bool {
    // Единый источник — таблицы allowed-efforts; matches! здесь дублировал бы CODEX/CLAUDE.
    provider_allowed_efforts(provider).contains(&effort)
}

pub(crate) fn provider_allowed_efforts(provider: &str) -> &'static [&'static str] {
    match provider {
        "codex" => CODEX_EFFORTS,
        "claude" => CLAUDE_EFFORTS,
        _ => COMMON_EFFORTS,
    }
}

pub(crate) fn effort_index_for(effort: &str) -> usize {
    EFFORTS
        .iter()
        .position(|value| *value == effort)
        .unwrap_or(2)
}

pub(crate) fn normalize_effort_index_for(allowed: &[&str], index: usize) -> usize {
    let effort = effort_label(index);
    if allowed.contains(&effort) {
        index
    } else {
        effort_index_for("high")
    }
}

pub(crate) fn normalize_common_effort_index(index: usize) -> usize {
    normalize_effort_index_for(COMMON_EFFORTS, index)
}

pub(crate) fn normalize_provider_effort_index(provider: &str, index: usize) -> usize {
    normalize_effort_index_for(provider_allowed_efforts(provider), index)
}

pub(crate) fn move_effort_index_in(allowed: &[&str], index: usize, direction: isize) -> usize {
    let normalized = normalize_effort_index_for(allowed, index);
    let current = effort_label(normalized);
    let current_pos = allowed
        .iter()
        .position(|effort| *effort == current)
        .unwrap_or_else(|| {
            allowed
                .iter()
                .position(|effort| *effort == "high")
                .unwrap_or(0)
        });
    let next_pos = if direction < 0 {
        current_pos.saturating_sub(1)
    } else {
        (current_pos + 1).min(allowed.len().saturating_sub(1))
    };
    effort_index_for(allowed[next_pos])
}

pub(crate) fn move_common_effort_index(index: usize, direction: isize) -> usize {
    move_effort_index_in(COMMON_EFFORTS, index, direction)
}

pub(crate) fn move_provider_effort_index(provider: &str, index: usize, direction: isize) -> usize {
    move_effort_index_in(provider_allowed_efforts(provider), index, direction)
}
