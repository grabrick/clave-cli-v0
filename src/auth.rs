use crate::prelude::*;
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

/// Проверяет логин ОДНОГО провайдера (для воркера, без заморозки UI).
pub(crate) fn provider_authenticated(provider: Provider) -> bool {
    match provider {
        Provider::Claude => claude_auth_probe().authenticated,
        Provider::Codex => codex_auth_probe().authenticated,
    }
}

/// Лёгкая проверка наличия бинарника провайдера в PATH — без запуска процесса,
/// поэтому безопасно звать из UI-потока (в отличие от `*_auth_probe`).
pub(crate) fn provider_binary_present(provider: &str) -> bool {
    let name = match provider {
        "claude" => claude_binary(),
        "codex" => codex_binary(),
        _ => return false,
    };
    if name.contains('/') {
        return std::path::Path::new(&name).is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(&name).is_file()))
}

pub(crate) fn codex_auth_probe() -> AuthProbe {
    match Command::new(codex_binary())
        .args(["login", "status"])
        .output()
    {
        Ok(output) => {
            let text = command_output_text(&output.stdout, &output.stderr);
            AuthProbe {
                installed: true,
                authenticated: auth_output_looks_ready(output.status.success(), &text),
                status: first_nonempty_line(&text)
                    .unwrap_or_else(|| "status unavailable".to_string()),
            }
        }
        Err(err) => AuthProbe {
            installed: false,
            authenticated: false,
            status: err.to_string(),
        },
    }
}

pub(crate) fn claude_auth_probe() -> AuthProbe {
    match Command::new(claude_binary())
        .args(["auth", "status", "--text"])
        .output()
    {
        Ok(output) => {
            let text = command_output_text(&output.stdout, &output.stderr);
            AuthProbe {
                installed: true,
                authenticated: auth_output_looks_ready(output.status.success(), &text),
                status: first_nonempty_line(&text)
                    .unwrap_or_else(|| "status unavailable".to_string()),
            }
        }
        Err(err) => AuthProbe {
            installed: false,
            authenticated: false,
            status: err.to_string(),
        },
    }
}

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

pub(crate) fn run_external_command(command: &ExternalCommand) -> AnyResult<i32> {
    // Живой блок инлайн (без alt-screen): просто отпускаем raw-режим и пишем под ним.
    disable_raw_mode()?;
    execute!(io::stdout(), crossterm::cursor::Show)?;

    println!();
    println!(
        "Clave: running {} {}",
        command.program,
        command.args.join(" ")
    );
    println!();

    let result = Command::new(command.program).args(command.args).status();
    let code = match result {
        Ok(status) => status.code().unwrap_or(1),
        Err(err) => {
            println!("Clave: failed to start command: {err}");
            1
        }
    };

    println!();
    println!("Clave: press Enter to return...");
    let mut wait = String::new();
    let _ = io::stdin().read_line(&mut wait);

    enable_raw_mode()?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

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
