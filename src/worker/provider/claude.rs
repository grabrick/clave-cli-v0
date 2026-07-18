use crate::prelude::*;
use crate::*;

/// Превратить claude tool_use в короткую человекочитаемую активность для лоадера.
fn summarize_claude_tool(item: &serde_json::Value, lang: Language) -> Option<String> {
    let name = item.get("name")?.as_str()?;
    let input = item.get("input");
    let path = input
        .and_then(|i| i.get("file_path"))
        .and_then(|v| v.as_str())
        .map(short_path);
    let command = input
        .and_then(|i| i.get("command"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let pattern = input
        .and_then(|i| i.get("pattern"))
        .and_then(|v| v.as_str());
    let summary = match name {
        "Read" | "NotebookRead" => {
            format!(
                "{} {}",
                lang.choose("Читаю", "Reading"),
                path.unwrap_or_default()
            )
        }
        "Edit" | "MultiEdit" | "NotebookEdit" => {
            format!(
                "{} {}",
                lang.choose("Правлю", "Editing"),
                path.unwrap_or_default()
            )
        }
        "Write" => format!(
            "{} {}",
            lang.choose("Создаю", "Writing"),
            path.unwrap_or_default()
        ),
        "Bash" => format!(
            "{} {}",
            lang.choose("Выполняю", "Running"),
            truncate_chars(command, 50)
        ),
        "Grep" => match pattern {
            Some(p) => format!(
                "{} {}",
                lang.choose("Ищу", "Searching"),
                truncate_chars(p, 40)
            ),
            None => lang.choose("Ищу по коду", "Searching code").to_string(),
        },
        "Glob" => match pattern {
            Some(p) => format!(
                "{} {}",
                lang.choose("Просматриваю", "Listing"),
                truncate_chars(p, 40)
            ),
            None => lang
                .choose("Просматриваю файлы", "Listing files")
                .to_string(),
        },
        other => format!("⚙ {other}"),
    };
    Some(summary)
}

/// Достаёт инкремент текста ответа из события claude (`--include-partial-messages`):
/// либо сам объект — `content_block_delta`, либо завёрнут в `stream_event.event`.
/// Берём только `text_delta` (не thinking/signature).
fn claude_text_delta(value: &serde_json::Value) -> Option<String> {
    let block = match value.get("type").and_then(|v| v.as_str()) {
        Some("stream_event") => value.get("event")?,
        _ => value,
    };
    if block.get("type").and_then(|v| v.as_str()) != Some("content_block_delta") {
        return None;
    }
    let delta = block.get("delta")?;
    if delta.get("type").and_then(|v| v.as_str()) != Some("text_delta") {
        return None;
    }
    delta
        .get("text")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// Дельта рассуждения (`thinking_delta`) из стрима claude — то же ограждение, что и
/// для текста ответа, но поле `thinking` вместо `text`. Пусто, если thinking выключен.
fn claude_thinking_delta(value: &serde_json::Value) -> Option<String> {
    let block = match value.get("type").and_then(|v| v.as_str()) {
        Some("stream_event") => value.get("event")?,
        _ => value,
    };
    if block.get("type").and_then(|v| v.as_str()) != Some("content_block_delta") {
        return None;
    }
    let delta = block.get("delta")?;
    if delta.get("type").and_then(|v| v.as_str()) != Some("thinking_delta") {
        return None;
    }
    delta
        .get("thinking")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// Начало нового text-блока в стриме claude (`content_block_start` с `content_block.type ==
/// "text"`). В многошаговом режиме (FullAccess) каждый шаг между `tool_use` открывает свой
/// text-блок; на их границе нужен разделитель абзаца, иначе тексты слипаются впритык.
fn claude_text_block_start(value: &serde_json::Value) -> bool {
    let block = match value.get("type").and_then(|v| v.as_str()) {
        Some("stream_event") => match value.get("event") {
            Some(event) => event,
            None => return false,
        },
        _ => value,
    };
    block.get("type").and_then(|v| v.as_str()) == Some("content_block_start")
        && block
            .get("content_block")
            .and_then(|c| c.get("type"))
            .and_then(|v| v.as_str())
            == Some("text")
}

/// Потоково читает claude stream-json: токены ответа эмитит как StreamDelta (живой
/// вывод), активность tool_use — в лоадер, и возвращает финальное result-событие.
pub(crate) fn spawn_claude_activity_reader(
    reader: impl Read + Send + 'static,
    tx: Sender<WorkerEvent>,
    lang: Language,
    last_activity: Arc<Mutex<Instant>>,
) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let reader = BufReader::new(reader);
        let mut result_line = String::new();
        // Был ли уже text-блок: на границе следующего вставляем разделитель абзаца, чтобы
        // тексты соседних шагов (между tool_use в FullAccess) не слипались впритык.
        let mut seen_text_block = false;
        for line in reader.lines().map_while(Result::ok) {
            touch_activity(&last_activity);
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if claude_text_block_start(&value) {
                if seen_text_block {
                    let _ = tx.send(WorkerEvent::StreamDelta("\n\n".to_string()));
                }
                seen_text_block = true;
                continue;
            }
            if let Some(delta) = claude_text_delta(&value) {
                let _ = tx.send(WorkerEvent::StreamDelta(delta));
                continue;
            }
            // Рассуждение (extended thinking при высоком effort) — отдельным потоком
            // в лоадер, чтобы было видно, как модель думает до ответа.
            if let Some(delta) = claude_thinking_delta(&value) {
                let _ = tx.send(WorkerEvent::ReasoningDelta(delta));
                continue;
            }
            match value.get("type").and_then(|v| v.as_str()) {
                Some("assistant") => {
                    if let Some(content) = value
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(|c| c.as_array())
                    {
                        for item in content {
                            if item.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                                if let Some(activity) = summarize_claude_tool(item, lang) {
                                    let _ = tx.send(WorkerEvent::Activity(activity));
                                }
                            }
                        }
                    }
                }
                Some("result") => result_line = line.clone(),
                _ => {}
            }
        }
        result_line
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_text_delta_extracts_only_streamed_answer_text() {
        // Текст-дельта, завёрнутая в stream_event → берём.
        let wrapped = serde_json::json!({"type":"stream_event","event":{
            "type":"content_block_delta","index":0,
            "delta":{"type":"text_delta","text":"привет"}}});
        assert_eq!(claude_text_delta(&wrapped).as_deref(), Some("привет"));
        // Размышления (thinking) и финальный result — НЕ стримим как ответ.
        let thinking = serde_json::json!({"type":"stream_event","event":{
            "type":"content_block_delta","delta":{"type":"thinking_delta","text":"гм"}}});
        assert_eq!(claude_text_delta(&thinking), None);
        assert_eq!(
            claude_text_delta(&serde_json::json!({"type":"result","result":"x"})),
            None
        );
    }

    #[test]
    fn claude_thinking_delta_extracts_reasoning_only() {
        // thinking_delta (поле `thinking`) → берём как рассуждение.
        let think = serde_json::json!({"type":"stream_event","event":{
            "type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"прикину"}}});
        assert_eq!(claude_thinking_delta(&think).as_deref(), Some("прикину"));
        // Текст ответа рассуждением НЕ считаем (он идёт своим потоком).
        let text = serde_json::json!({"type":"stream_event","event":{
            "type":"content_block_delta","delta":{"type":"text_delta","text":"ответ"}}});
        assert_eq!(claude_thinking_delta(&text), None);
    }

    #[test]
    fn adjacent_turn_text_blocks_get_a_paragraph_break() {
        // FullAccess: между шагами (tool_use) claude открывает НОВЫЙ text-блок. Их тексты
        // обязаны разделяться абзацем — иначе «файл.Файл» слипается впритык (обкатка BUG-002).
        let jsonl = [
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Читаю файл."}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Файл про кошек."}}}"#,
        ]
        .join("\n");
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = spawn_claude_activity_reader(
            std::io::Cursor::new(jsonl),
            tx,
            Language::Ru,
            std::sync::Arc::new(std::sync::Mutex::new(std::time::Instant::now())),
        );
        handle.join().unwrap();
        let streamed: String = rx
            .into_iter()
            .filter_map(|e| match e {
                WorkerEvent::StreamDelta(s) => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(
            streamed, "Читаю файл.\n\nФайл про кошек.",
            "тексты соседних turn-блоков должны разделяться абзацем, а не слипаться"
        );
    }

    #[test]
    fn summarizes_claude_tool_use() {
        let read = serde_json::json!({
            "type": "tool_use",
            "name": "Read",
            "input": {"file_path": "/Users/x/proj/src/model/overlay.rs"}
        });
        assert_eq!(
            summarize_claude_tool(&read, Language::En),
            Some("Reading model/overlay.rs".to_string())
        );

        let bash = serde_json::json!({
            "type": "tool_use",
            "name": "Bash",
            "input": {"command": "cargo build"}
        });
        assert_eq!(
            summarize_claude_tool(&bash, Language::En),
            Some("Running cargo build".to_string())
        );

        let write = serde_json::json!({
            "type": "tool_use",
            "name": "Write",
            "input": {"file_path": "/a/b/new.rs"}
        });
        assert_eq!(
            summarize_claude_tool(&write, Language::En),
            Some("Writing b/new.rs".to_string())
        );

        let grep = serde_json::json!({
            "type": "tool_use",
            "name": "Grep",
            "input": {"pattern": "TODO"}
        });
        assert_eq!(
            summarize_claude_tool(&grep, Language::En),
            Some("Searching TODO".to_string())
        );
    }
}
