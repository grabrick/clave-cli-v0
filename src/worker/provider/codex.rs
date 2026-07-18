use crate::prelude::*;
use crate::*;

/// Активность из события codex `item.started`: команду показываем детально, прочие
/// типы (reasoning, agent_message, file_change, …) — обобщённо, чтобы codex-прогон
/// не выглядел «просто спиннером» (раньше активность была только для команд).
fn codex_activity(value: &serde_json::Value, lang: Language) -> Option<String> {
    if value.get("type")?.as_str()? != "item.started" {
        return None;
    }
    let item = value.get("item")?;
    match item.get("type").and_then(|v| v.as_str()).unwrap_or("") {
        "command_execution" => item
            .get("command")
            .and_then(|v| v.as_str())
            .map(|command| summarize_codex_command(command, lang)),
        "reasoning" => Some(lang.choose("Рассуждаю…", "Reasoning…").to_string()),
        "agent_message" | "assistant_message" => {
            Some(lang.choose("Пишу ответ…", "Writing answer…").to_string())
        }
        "file_change" | "patch" => Some(lang.choose("Правлю файлы", "Editing files").to_string()),
        "mcp_tool_call" | "tool_call" => Some(
            lang.choose("Вызываю инструмент", "Calling a tool")
                .to_string(),
        ),
        "" => None,
        other => Some(format!("⚙ {other}")),
    }
}

fn codex_path_token(command: &str) -> Option<String> {
    command
        .split_whitespace()
        .rev()
        .map(|token| token.trim_matches(|c| c == '"' || c == '\''))
        .find(|token| token.contains('/') || token.contains('.'))
        .map(String::from)
}

/// Превратить shell-команду codex в короткую человекочитаемую активность для лоадера.
pub(crate) fn summarize_codex_command(command: &str, lang: Language) -> String {
    let inner = command
        .split_once("-lc")
        .map(|(_, rest)| rest.trim().trim_matches('"').trim().to_string())
        .unwrap_or_else(|| command.to_string());
    let first = inner.split_whitespace().next().unwrap_or("").to_lowercase();

    if matches!(
        first.as_str(),
        "sed" | "cat" | "head" | "tail" | "less" | "bat" | "more"
    ) {
        return match codex_path_token(&inner) {
            Some(file) => format!("{} {}", lang.choose("Читаю", "Reading"), file),
            None => lang.choose("Читаю файл", "Reading file").to_string(),
        };
    }
    if matches!(first.as_str(), "grep" | "rg" | "ag" | "ack") {
        return lang.choose("Ищу по коду", "Searching code").to_string();
    }
    if matches!(first.as_str(), "ls" | "find" | "fd" | "tree") {
        return lang
            .choose("Просматриваю файлы", "Listing files")
            .to_string();
    }
    format!("⚙ {}", truncate_chars(&inner, 60))
}

/// Потоково читает JSONL codex: эмитит активность (command_execution) в лоадер
/// и возвращает весь stdout (для разбора usage в конце).
pub(crate) fn spawn_codex_activity_reader(
    reader: impl Read + Send + 'static,
    tx: Sender<WorkerEvent>,
    lang: Language,
    last_activity: Arc<Mutex<Instant>>,
) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let reader = BufReader::new(reader);
        let mut full = String::new();
        for line in reader.lines().map_while(Result::ok) {
            touch_activity(&last_activity);
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                if let Some(activity) = codex_activity(&value, lang) {
                    let _ = tx.send(WorkerEvent::Activity(activity));
                }
            }
            full.push_str(&line);
            full.push('\n');
        }
        full
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_activity_covers_item_types() {
        let item = |t: &str| serde_json::json!({"type":"item.started","item":{"type":t}});
        // Не item.started → нет активности.
        assert!(
            codex_activity(&serde_json::json!({"type":"turn.started"}), Language::Ru).is_none()
        );
        assert_eq!(
            codex_activity(&item("reasoning"), Language::Ru).as_deref(),
            Some("Рассуждаю…")
        );
        assert_eq!(
            codex_activity(&item("agent_message"), Language::Ru).as_deref(),
            Some("Пишу ответ…")
        );
        // Команда — детальная сводка (не дефолтная заглушка).
        let cmd = serde_json::json!({"type":"item.started",
            "item":{"type":"command_execution","command":"ls -la"}});
        assert!(codex_activity(&cmd, Language::Ru).is_some());
        // Неизвестный тип — обобщённо, но не молчим (codex не должен быть «спиннером»).
        assert_eq!(
            codex_activity(&item("totally_new"), Language::Ru).as_deref(),
            Some("⚙ totally_new")
        );
    }

    #[test]
    fn summarizes_codex_read_command() {
        let cmd = "/bin/zsh -lc \"sed -n '1,240p' src/model/overlay.rs\"";
        assert_eq!(
            summarize_codex_command(cmd, Language::En),
            "Reading src/model/overlay.rs"
        );
        let grep = "/bin/zsh -lc \"grep -rn Overlay src\"";
        assert_eq!(
            summarize_codex_command(grep, Language::En),
            "Searching code"
        );
    }

    #[test]
    fn codex_command_summary_uses_path_like_token() {
        // Токен пути — со слэшем ИЛИ с точкой: имя файла рядом тоже путь.
        assert_eq!(
            summarize_codex_command("cat notes.md", Language::En),
            "Reading notes.md"
        );
        assert_eq!(
            summarize_codex_command("cat README", Language::En),
            "Reading file"
        );
    }
}
