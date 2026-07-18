use crate::prelude::*;
use crate::*;

pub(crate) fn estimate_tokens(text: &str) -> usize {
    let chars = text.chars().count();
    let words = text.split_whitespace().count();
    ((chars / 4).max(words)).max(1)
}

pub(crate) fn format_token_count(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}m", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

pub(crate) fn provider_display(provider: &str, lang: Language) -> &'static str {
    match provider {
        "codex" => "Codex",
        "claude" => "Claude",
        _ => lang.choose("Модель", "Model"),
    }
}

pub(crate) struct ChatResponse {
    pub(crate) text: String,
    pub(crate) usage: Option<RunUsage>,
    pub(crate) is_error: bool,
}

/// Разобрать ответ `claude -p --output-format json`. При невалидном JSON —
/// fallback: весь stdout как текст, usage отсутствует.
pub(crate) fn parse_claude_response(stdout: &str) -> ChatResponse {
    let trimmed = stdout.trim();
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(value) => {
            let text = value
                .get("result")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let is_error = value
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let usage = value.get("usage").map(|u| RunUsage {
                input: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                output: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                cache_read: u
                    .get("cache_read_input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                cache_creation: u
                    .get("cache_creation_input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                cost_usd: value
                    .get("total_cost_usd")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0),
            });
            ChatResponse {
                text,
                usage,
                is_error,
            }
        }
        Err(_) => ChatResponse {
            text: trimmed.to_string(),
            usage: None,
            is_error: false,
        },
    }
}

/// Рекурсивно ищем объект с токенами (имена полей различаются между версиями codex).
fn find_token_usage(value: &serde_json::Value) -> Option<RunUsage> {
    let input = value
        .get("input_tokens")
        .or_else(|| value.get("prompt_tokens"))
        .and_then(|v| v.as_u64());
    let output = value
        .get("output_tokens")
        .or_else(|| value.get("completion_tokens"))
        .and_then(|v| v.as_u64());
    if let (Some(input), Some(output)) = (input, output) {
        let cache_read = value
            .get("cached_input_tokens")
            .or_else(|| value.get("cache_read_input_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        return Some(RunUsage {
            input,
            output,
            cache_read,
            cache_creation: 0,
            cost_usd: 0.0,
        });
    }
    match value {
        serde_json::Value::Object(map) => map.values().find_map(find_token_usage),
        serde_json::Value::Array(items) => items.iter().find_map(find_token_usage),
        _ => None,
    }
}

/// Разобрать JSONL событий `codex exec --json`, вернуть последний найденный usage.
/// codex не сообщает стоимость, поэтому cost_usd = 0.0.
pub(crate) fn parse_codex_usage(jsonl: &str) -> Option<RunUsage> {
    let mut last = None;
    for line in jsonl.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(usage) = find_token_usage(&value) {
                last = Some(usage);
            }
        }
    }
    last
}

pub(crate) fn emit_chat_lines(tx: &Sender<WorkerEvent>, text: &str) {
    let mut first_content = true;
    for line in text.lines() {
        let rendered = if first_content && !line.trim().is_empty() {
            first_content = false;
            format!("⏺ {}", line.trim_start())
        } else {
            line.to_string()
        };
        let _ = tx.send(WorkerEvent::ChatLine(rendered));
    }
}

/// Строки для показа при ошибке провайдера в чате: заголовок с КОДОМ выхода, затем
/// детали из stderr — а если stderr пуст (claude шлёт ошибки в stdout stream-json и
/// при обрыве до `result` они не доезжают), честная подсказка о транзиентной природе
/// сбоя вместо немого «no stderr output».
pub(crate) fn chat_error_lines(
    provider: &'static str,
    code: i32,
    stderr: &str,
    lang: Language,
) -> Vec<String> {
    let mut out = vec![format!(
        "{} {} ({} {code}):",
        provider_display(provider, lang),
        lang.choose("вернул ошибку", "returned an error"),
        lang.choose("код", "exit code"),
    )];
    for line in provider_error_detail_lines(stderr, lang) {
        out.push(format!("⎿ {line}"));
    }
    out
}

/// Детали ошибки провайдера: непустой stderr (первые 40 непустых строк) либо честная
/// подсказка о транзиенте, если stderr пуст. Общий источник для чат-ошибки (`chat_error_lines`)
/// И для шага тандема (`run_provider_once`) — иначе тандем отдаёт пустой текст, и «код N»
/// остаётся без причины.
pub(crate) fn provider_error_detail_lines(stderr: &str, lang: Language) -> Vec<String> {
    let stderr = stderr.trim();
    if stderr.is_empty() {
        vec![lang
            .choose(
                "без вывода — вероятно транзиентный сбой (сеть / лимит / таймаут). Повтори запрос.",
                "no output — likely a transient failure (network / rate limit / timeout). Try again.",
            )
            .to_string()]
    } else {
        stderr
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(40)
            .map(str::to_string)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_claude_json_with_usage() {
        let raw = r#"{"type":"result","is_error":false,"result":"Привет!","total_cost_usd":0.0123,"usage":{"input_tokens":120,"output_tokens":40,"cache_read_input_tokens":5,"cache_creation_input_tokens":9}}"#;
        let parsed = parse_claude_response(raw);
        assert_eq!(parsed.text, "Привет!");
        assert!(!parsed.is_error);
        let usage = parsed.usage.expect("usage present");
        assert_eq!(usage.input, 120);
        assert_eq!(usage.output, 40);
        assert_eq!(usage.cache_read, 5);
        assert_eq!(usage.cache_creation, 9);
        assert!((usage.cost_usd - 0.0123).abs() < 1e-9);
    }

    #[test]
    fn claude_parser_falls_back_on_non_json() {
        let parsed = parse_claude_response("просто текст без json");
        assert_eq!(parsed.text, "просто текст без json");
        assert!(parsed.usage.is_none());
    }

    #[test]
    fn parses_codex_usage_from_jsonl() {
        let jsonl = "{\"type\":\"item\",\"text\":\"hi\"}\n{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":200,\"output_tokens\":60,\"cached_input_tokens\":10}}\n";
        let usage = parse_codex_usage(jsonl).expect("usage found");
        assert_eq!(usage.input, 200);
        assert_eq!(usage.output, 60);
        assert_eq!(usage.cache_read, 10);
        assert_eq!(usage.cost_usd, 0.0);
    }

    #[test]
    fn codex_usage_none_when_absent() {
        let jsonl = "{\"type\":\"item\",\"text\":\"hi\"}\n";
        assert!(parse_codex_usage(jsonl).is_none());
    }

    #[test]
    fn chat_error_lines_surface_code_and_cause() {
        // Пустой stderr (типичный claude-сбой): заголовок с кодом + подсказка о
        // транзиентной причине, БЕЗ немого «no stderr output».
        let lines = chat_error_lines("claude", 1, "  ", Language::Ru);
        assert!(
            lines[0].contains("код 1"),
            "код выхода в заголовке: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("транзиентный")),
            "подсказка о причине: {lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("no stderr output")),
            "немого сообщения больше нет"
        );

        // Непустой stderr (codex): код + строки stderr, без подсказки-заглушки.
        let lines = chat_error_lines("codex", 2, "boom: connection reset\n", Language::En);
        assert!(lines[0].contains("exit code 2"), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("boom: connection reset")));
        assert!(!lines.iter().any(|l| l.contains("transient")));
    }

    #[test]
    fn estimate_tokens_takes_the_larger_estimate() {
        assert_eq!(estimate_tokens(""), 1, "пустой текст — не ноль токенов");
        assert_eq!(estimate_tokens("abcdefgh"), 2, "8 символов / 4");
        assert_eq!(
            estimate_tokens("a b c d e"),
            5,
            "короткие слова: побеждают слова"
        );
        assert_eq!(
            estimate_tokens(&"я".repeat(40)),
            10,
            "считаем символы, а не байты"
        );
    }

    #[test]
    fn format_token_count_switches_units_on_thresholds() {
        assert_eq!(format_token_count(999), "999");
        assert_eq!(format_token_count(1_000), "1.0k");
        assert_eq!(format_token_count(999_999), "1000.0k");
        assert_eq!(format_token_count(1_000_000), "1.0m");
        assert_eq!(format_token_count(2_500_000), "2.5m");
    }

    #[test]
    fn provider_display_names_each_provider() {
        assert_eq!(provider_display("codex", Language::Ru), "Codex");
        assert_eq!(provider_display("claude", Language::Ru), "Claude");
        assert_eq!(provider_display("gpt", Language::Ru), "Модель");
        assert_eq!(provider_display("gpt", Language::En), "Model");
    }

    #[test]
    fn codex_usage_is_found_inside_arrays() {
        let jsonl = "{\"items\":[{\"usage\":{\"input_tokens\":4,\"output_tokens\":1}}]}\n";
        let usage = parse_codex_usage(jsonl).expect("usage найден внутри массива");
        assert_eq!(usage.input, 4);
        assert_eq!(usage.output, 1);
    }

    // --- накопление и лента тандема ---

    #[test]
    fn provider_error_detail_surfaces_stderr_or_transient_hint() {
        // Непустой stderr — показываем его строки (пустые отбрасываем). Это то, что раньше
        // тандем выбрасывал: причина провала фазы исполнения жила ровно здесь.
        let detail = provider_error_detail_lines(
            "\nError: permission denied\n\n/path/key-guard.ts\n",
            Language::Ru,
        );
        assert_eq!(
            detail,
            vec!["Error: permission denied", "/path/key-guard.ts"]
        );

        // Пустой stderr — честная подсказка о транзиенте, а не немой провал.
        let empty = provider_error_detail_lines("   ", Language::Ru);
        assert_eq!(empty.len(), 1);
        assert!(
            empty[0].contains("транзиентный сбой"),
            "пустой stderr → подсказка о транзиенте: {empty:?}"
        );

        // chat_error_lines строит поверх того же помощника: заголовок с кодом + детали.
        let lines = chat_error_lines("claude", 1, "boom", Language::Ru);
        assert!(lines[0].contains("код 1"), "{lines:?}");
        assert!(lines.iter().any(|l| l == "⎿ boom"), "{lines:?}");
    }
}
