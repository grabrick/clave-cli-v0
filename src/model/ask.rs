use serde_json::Value;

/// Один вариант выбора: подпись + необязательная аннотация (пояснение).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AskOption {
    pub(crate) label: String,
    pub(crate) note: Option<String>,
}

/// Один вопрос с вариантами.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AskQuestion {
    pub(crate) question: String,
    pub(crate) multi: bool,
    pub(crate) options: Vec<AskOption>,
    /// Показывать ли строку «Свой ответ». Для вопросов модели — да. Для локальных вопросов
    /// самого приложения (например «включить зрение?») — нет: свободный текст там некому
    /// обработать, и строка была бы мёртвой — Enter на ней не делал бы ничего.
    pub(crate) allow_custom: bool,
}

/// Разобранный запрос выбора (блок ```clave-ask): один или несколько вопросов.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AskPrompt {
    pub(crate) questions: Vec<AskQuestion>,
}

/// Открывающее ограждение блока выбора. Один источник истины для парсера и для
/// живого стрима (чтобы прятать JSON одинаково).
pub(crate) const ASK_FENCE: &str = "```clave-ask";

struct AskBlock {
    /// Смещение начала строки с открывающим маркером (чтобы вырезать блок целиком).
    start: usize,
    json: String,
}

/// Видимая часть стримящегося ответа: всё до блока ```clave-ask`.
///
/// Пока ответ печатается по токенам, тело блока (JSON выбора) не должно мелькать
/// в ленте — панель откроется только на `ChatDone`. Режем по началу строки с
/// маркером. Любая строка, начинающаяся с ``` , и так не отрисовывается (это
/// ограждение), а тело JSON идёт уже ПОСЛЕ полного маркера — поэтому обрезка по
/// маркеру прячет JSON и при этом никогда не съедает обычные блоки кода.
pub(crate) fn live_answer_visible(text: &str) -> &str {
    // Маркер — настоящее ограждение только в начале строки. Идём по началам строк
    // (после каждого '\n' — ASCII, граница символа сохраняется).
    let mut from = 0;
    loop {
        if text[from..].starts_with(ASK_FENCE) {
            return text[..from].trim_end();
        }
        match text[from..].find('\n') {
            Some(rel) => from += rel + 1,
            None => return text,
        }
    }
}

/// Находит первый блок ```clave-ask … ``` и парсит его JSON.
///
/// Возвращает `(текст_без_блока, Some(prompt))` при успехе. Если блока нет, JSON
/// невалиден или вариантов меньше двух — `(исходный_текст, None)`: блок остаётся
/// обычным текстом (мягкий фолбэк). Парсить нужно СЫРОЙ ответ модели, до того как
/// UI вешает префикс «⏺» и прячет ограждение ```.
pub(crate) fn parse_clave_ask(text: &str) -> (String, Option<AskPrompt>) {
    let Some(block) = find_ask_block(text) else {
        return (text.to_string(), None);
    };
    let Some(prompt) = parse_ask_json(&block.json) else {
        return (text.to_string(), None);
    };
    // Проза до блока сохраняется; хвост после блока в V0 отбрасываем.
    let prose = text[..block.start].trim_end().to_string();
    (prose, Some(prompt))
}

fn find_ask_block(text: &str) -> Option<AskBlock> {
    let open = text.find(ASK_FENCE)?;
    let line_start = text[..open].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let after_marker = open + ASK_FENCE.len();
    // тело начинается со следующей строки после маркера
    let body_start = text[after_marker..]
        .find('\n')
        .map(|i| after_marker + i + 1)?;
    let close_rel = text[body_start..].find("```")?;
    let json = text[body_start..body_start + close_rel].trim().to_string();
    Some(AskBlock {
        start: line_start,
        json,
    })
}

/// Истинно ли значение `multi` (терпимо к bool / "true" / 1 / "yes" / "да").
fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::String(s) => matches!(
            s.trim().to_ascii_lowercase().as_str(),
            "true" | "1" | "yes" | "да"
        ),
        Value::Number(n) => n.as_i64() == Some(1) || n.as_f64() == Some(1.0),
        _ => false,
    }
}

fn parse_ask_json(json: &str) -> Option<AskPrompt> {
    let value: Value = serde_json::from_str(json).ok()?;
    // Форма с несколькими вопросами: {"questions":[ {...}, ... ]}.
    if let Some(arr) = value.get("questions").and_then(Value::as_array) {
        let questions: Vec<AskQuestion> = arr.iter().filter_map(parse_question).collect();
        return (!questions.is_empty()).then_some(AskPrompt { questions });
    }
    // Одиночная форма: {question, multi, options}.
    let question = parse_question(&value)?;
    Some(AskPrompt {
        questions: vec![question],
    })
}

fn parse_question(value: &Value) -> Option<AskQuestion> {
    let question = value.get("question")?.as_str()?.trim().to_string();
    if question.is_empty() {
        return None;
    }
    // Терпимо и к названию поля, и к значению: модели пишут его по-разному
    // (multi / multiple / multiSelect / multi_select) и не всегда булевым ("true", 1).
    let multi = value.as_object().is_some_and(|obj| {
        obj.iter()
            .any(|(key, val)| key.to_ascii_lowercase().contains("multi") && is_truthy(val))
    });
    let options_val = value.get("options")?.as_array()?;

    let mut options = Vec::new();
    for opt in options_val {
        let Some(label) = opt
            .get("label")
            .and_then(Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let note = opt
            .get("note")
            .and_then(Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        options.push(AskOption { label, note });
    }

    // Вопрос с выбором имеет смысл только при ≥2 вариантах — иначе фолбэк в текст.
    (options.len() >= 2).then_some(AskQuestion {
        question,
        multi,
        options,
        allow_custom: true, // вопрос от модели — свободный ответ ей и уйдёт
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(json: &str) -> String {
        format!("```clave-ask\n{json}\n```")
    }

    #[test]
    fn parses_single_select_with_notes() {
        let text = block(
            r#"{"question":"Какой подход?","multi":false,
                "options":[{"label":"JWT","note":"stateless"},{"label":"Сессии"}]}"#,
        );
        let (prose, prompt) = parse_clave_ask(&text);
        let prompt = prompt.expect("валидный блок → Some");
        assert_eq!(prose, "", "блок без прозы → пустой текст");
        assert_eq!(prompt.questions.len(), 1);
        let q = &prompt.questions[0];
        assert_eq!(q.question, "Какой подход?");
        assert!(!q.multi);
        assert_eq!(q.options.len(), 2);
        assert_eq!(q.options[0].label, "JWT");
        assert_eq!(q.options[0].note.as_deref(), Some("stateless"));
        assert_eq!(q.options[1].note, None, "note необязателен");
    }

    #[test]
    fn parses_multi_select() {
        let text = block(
            r#"{"question":"Что включить?","multi":true,
            "options":[{"label":"A"},{"label":"B"},{"label":"C"}]}"#,
        );
        let (_, prompt) = parse_clave_ask(&text);
        let prompt = prompt.expect("Some");
        assert_eq!(prompt.questions.len(), 1);
        assert!(prompt.questions[0].multi);
        assert_eq!(prompt.questions[0].options.len(), 3);
    }

    #[test]
    fn parses_several_questions() {
        let text = block(
            r#"{"questions":[
                {"question":"БД?","options":[{"label":"PG"},{"label":"SQLite"}]},
                {"question":"Кэш?","multi":true,"options":[{"label":"Redis"},{"label":"Memcached"}]}
            ]}"#,
        );
        let (_, prompt) = parse_clave_ask(&text);
        let prompt = prompt.expect("Some");
        assert_eq!(prompt.questions.len(), 2);
        assert_eq!(prompt.questions[0].question, "БД?");
        assert!(!prompt.questions[0].multi);
        assert!(prompt.questions[1].multi);
    }

    #[test]
    fn multi_is_parsed_leniently() {
        // Истинно: разные имена поля и разные формы значения.
        for field in ["multi", "multiple", "multiSelect", "multi_select"] {
            for raw in [r#""true""#, "1", r#""yes""#, "true"] {
                let text = block(&format!(
                    r#"{{"question":"q","{field}":{raw},"options":[{{"label":"A"}},{{"label":"B"}}]}}"#
                ));
                let (_, prompt) = parse_clave_ask(&text);
                assert!(
                    prompt.expect("Some").questions[0].multi,
                    "{field}={raw} должно быть true"
                );
            }
        }
        // Ложно: явный false / 0 / null / отсутствие поля — одиночный.
        for raw in [r#""false""#, "0", "null"] {
            let text = block(&format!(
                r#"{{"question":"q","multi":{raw},"options":[{{"label":"A"}},{{"label":"B"}}]}}"#
            ));
            let (_, prompt) = parse_clave_ask(&text);
            assert!(
                !prompt.expect("Some").questions[0].multi,
                "multi={raw} должно быть false"
            );
        }
        let no_field = block(r#"{"question":"q","options":[{"label":"A"},{"label":"B"}]}"#);
        assert!(!parse_clave_ask(&no_field).1.expect("Some").questions[0].multi);
    }

    #[test]
    fn keeps_prose_before_block_and_strips_block() {
        let text = format!(
            "Рассуждение по теме.\nЕщё строка.\n{}",
            block(r#"{"question":"Какую БД?","options":[{"label":"PG"},{"label":"SQLite"}]}"#,)
        );
        let (prose, prompt) = parse_clave_ask(&text);
        assert_eq!(prose, "Рассуждение по теме.\nЕщё строка.");
        assert!(prompt.is_some());
        assert!(!prose.contains("clave-ask"), "блок вырезан из прозы");
    }

    #[test]
    fn fewer_than_two_options_is_fallback_to_text() {
        let text = block(r#"{"question":"?","options":[{"label":"Один"}]}"#);
        let (prose, prompt) = parse_clave_ask(&text);
        assert!(prompt.is_none(), "<2 вариантов → не селектор");
        assert_eq!(prose, text, "текст не тронут (фолбэк)");
    }

    #[test]
    fn malformed_json_is_fallback_to_text() {
        let text = block(r#"{"question": "битый", "options": [ {"label": }"#);
        let (prose, prompt) = parse_clave_ask(&text);
        assert!(prompt.is_none());
        assert_eq!(prose, text);
    }

    #[test]
    fn plain_text_without_block_is_unchanged() {
        let text = "Обычный ответ без выбора.";
        let (prose, prompt) = parse_clave_ask(text);
        assert!(prompt.is_none());
        assert_eq!(prose, text);
    }

    #[test]
    fn live_stream_hides_ask_block_keeps_prose_and_code() {
        // Проза до блока — видна; тело блока (JSON) — спрятано целиком.
        let streamed = "Сейчас уточню.\n```clave-ask\n{\"question\":\"q\",\"options\":[";
        assert_eq!(live_answer_visible(streamed), "Сейчас уточню.");

        // Блок в самом начале → видимого текста нет (рисуется только лоадер).
        assert_eq!(live_answer_visible("```clave-ask\n{\"que"), "");

        // Обычный блок кода не трогаем — режем только clave-ask.
        let code = "Вот пример:\n```rust\nfn main() {}\n```";
        assert_eq!(live_answer_visible(code), code);

        // Без блока текст возвращается как есть.
        assert_eq!(live_answer_visible("просто ответ"), "просто ответ");

        // Маркер на середине строки (не с начала) — это не ограждение, не режем.
        let inline = "см. ```clave-ask внутри строки";
        assert_eq!(live_answer_visible(inline), inline);
    }

    #[test]
    fn empty_labels_are_skipped_then_validated() {
        // Два валидных варианта + один пустой label → пустой пропускаем, остаётся 2.
        let text =
            block(r#"{"question":"q","options":[{"label":"A"},{"label":"  "},{"label":"B"}]}"#);
        let (_, prompt) = parse_clave_ask(&text);
        let q = &prompt.expect("Some").questions[0];
        assert_eq!(q.options.len(), 2);
        assert_eq!(q.options[1].label, "B");
    }
}
