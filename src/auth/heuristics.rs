/// Эвристика «залогинен ли провайдер» по выводу `*_auth_probe`. Стабильного контракта у
/// чужих CLI нет, поэтому по приоритету: явные маркеры НЕ-логина перекрывают всё (их
/// печатают и с exit 0), затем явный маркер логина (принимаем даже при ненулевом коде —
/// бывает диагностический вывод), иначе — доверяемся коду выхода пробы.
pub(crate) fn auth_output_looks_ready(success: bool, text: &str) -> bool {
    let lower = text.to_lowercase();

    if AUTH_NOT_READY_MARKERS.iter().any(|m| lower.contains(m)) {
        return false;
    }
    if AUTH_READY_MARKERS.iter().any(|m| lower.contains(m)) {
        return true;
    }
    success
}

const AUTH_NOT_READY_MARKERS: &[&str] = &[
    "not logged",
    "not authenticated",
    "not signed",
    "login required",
    "please log in",
    "please login",
    "logged out",
    "no credentials",
    "unauthenticated",
    "auth required",
];

const AUTH_READY_MARKERS: &[&str] = &[
    "logged in",
    "logged into",
    "signed in",
    "authenticated as",
    "account:",
];

pub(crate) fn command_output_text(stdout: &[u8], stderr: &[u8]) -> String {
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(stdout));
    if !stderr.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&String::from_utf8_lossy(stderr));
    }
    text
}

pub(crate) fn first_nonempty_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("WARNING:"))
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_output_text_joins_streams_without_stray_newlines() {
        assert_eq!(command_output_text(b"out", b"err"), "out\nerr");
        assert_eq!(command_output_text(b"", b"err"), "err");
        assert_eq!(command_output_text(b"out", b""), "out");
        assert_eq!(command_output_text(b"", b""), "");
    }

    #[test]
    fn first_nonempty_line_skips_blanks_and_warnings() {
        assert_eq!(
            first_nonempty_line("   \n\n  WARNING: skip me\n  status: ok  \n"),
            Some("status: ok".to_string())
        );
        assert_eq!(first_nonempty_line("  \n\n"), None);
        assert_eq!(first_nonempty_line("WARNING: only"), None);
        assert_eq!(first_nonempty_line(""), None);
    }

    #[test]
    fn auth_ready_prefers_explicit_markers_over_exit_code() {
        // Явный не-логин перекрывает даже успешный код выхода.
        assert!(!auth_output_looks_ready(true, "You are not logged in."));
        assert!(!auth_output_looks_ready(true, "Login required"));
        // Явный логин принимается даже при ненулевом коде.
        assert!(auth_output_looks_ready(
            false,
            "Logged in as user@example.com"
        ));
        assert!(auth_output_looks_ready(false, "Authenticated as acme"));
        // Нейтральный вывод — доверяемся коду выхода пробы.
        assert!(auth_output_looks_ready(true, "status: ok"));
        assert!(!auth_output_looks_ready(false, "status: ok"));
        // «not authenticated» не должен приниматься маркером «authenticated as».
        assert!(!auth_output_looks_ready(true, "user is not authenticated"));
    }
}
