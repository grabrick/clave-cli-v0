use crate::prelude::*;
use crate::*;

pub(crate) fn final_brief_lines_for_chat(path: &str, lang: Language) -> io::Result<Vec<String>> {
    let content = fs::read_to_string(path)?;
    let mut lines = Vec::new();
    let mut in_current_spec = false;
    let mut in_last_review = false;
    let mut emitted_any = false;

    for raw in content.lines() {
        let line = raw.trim_end();
        if line == "## Current Spec" {
            in_current_spec = true;
            in_last_review = false;
            lines.push(
                lang.choose("## Текущая спека", "## Current Spec")
                    .to_string(),
            );
            emitted_any = true;
            continue;
        }
        if line == "## Last Review" {
            in_current_spec = false;
            in_last_review = true;
            lines.push(
                lang.choose("## Последнее ревью", "## Last Review")
                    .to_string(),
            );
            emitted_any = true;
            continue;
        }
        if line.starts_with("## ") {
            in_current_spec = false;
            in_last_review = false;
        }

        if in_current_spec || in_last_review {
            lines.push(line.to_string());
        }
    }

    if !emitted_any
        || lines
            .iter()
            .all(|line| line.trim().is_empty() || line.starts_with("## "))
    {
        lines = content.lines().map(ToString::to_string).collect();
    }

    let mut compact = Vec::new();
    let mut previous_blank = false;
    for line in lines {
        let blank = line.trim().is_empty();
        if blank && previous_blank {
            continue;
        }
        previous_blank = blank;
        compact.push(truncate_chars(&line, 220));
        if compact.len() >= 140 {
            compact.push(
                lang.choose(
                    "… ответ обрезан, полный brief сохранён в файле выше",
                    "… answer truncated, full brief is saved in the file above",
                )
                .to_string(),
            );
            break;
        }
    }

    Ok(compact)
}

pub(crate) fn is_welcome_line(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("✦ Добро пожаловать")
        || line.starts_with("✦ Welcome")
        || line.starts_with("Введите задачу")
        || line.starts_with("Type a task")
        || line.starts_with("Это Claude Code-style")
        || line.starts_with("This is a Claude Code-style")
}

pub(crate) fn truncate_chars(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }

    if max_chars == 0 {
        return String::new();
    }

    let mut truncated = text
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

pub(crate) fn first_prompt_title(lines: &[String]) -> Option<String> {
    lines
        .iter()
        .find_map(|line| line.strip_prefix("◆ ").map(str::trim))
        .filter(|line| !line.is_empty())
        .map(|line| truncate_chars(line, 72))
}

pub(crate) fn clave_state_dir() -> PathBuf {
    if let Ok(path) = env::var("CLAVE_HOME") {
        return PathBuf::from(path);
    }

    default_home_state_dir(STATE_DIR_NAME).unwrap_or_else(|| PathBuf::from(STATE_DIR_NAME))
}

fn default_home_state_dir(name: &str) -> Option<PathBuf> {
    env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(name))
}

pub(crate) fn history_path() -> PathBuf {
    clave_state_dir().join("history")
}

pub(crate) fn chats_dir() -> PathBuf {
    clave_state_dir().join("chats")
}

pub(crate) fn config_path() -> PathBuf {
    if let Ok(path) = env::var("CLAVE_CONFIG") {
        return PathBuf::from(path);
    }

    clave_state_dir().join("config")
}

pub(crate) fn load_config(path: &Path) -> AppConfig {
    let Ok(content) = fs::read_to_string(path) else {
        return AppConfig::default();
    };

    let mut config = AppConfig::default();
    let mut legacy_effort = None;
    let mut codex_effort_seen = false;
    let mut claude_effort_seen = false;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"');

        match key {
            "onboarding_done" => config.onboarding_done = value == "true",
            "mode" => {
                if let Some(mode) = Mode::from_str(value) {
                    config.mode = mode;
                }
            }
            "direct_provider" | "chat_provider" | "direct_chat_provider" => {
                if let Some(provider) = Provider::from_str(value) {
                    config.direct_provider = provider;
                }
            }
            "theme" | "color_theme" | "palette" => {
                if let Some(theme) = Theme::from_str(value) {
                    config.theme = theme;
                }
            }
            "lang" => {
                if let Some(lang) = Language::from_str(value) {
                    config.lang = lang;
                }
            }
            "rounds" => {
                if let Ok(rounds) = value.parse::<usize>() {
                    config.rounds = rounds.max(1);
                }
            }
            "work_dir" | "cwd" => config.work_dir = value.to_string(),
            "out_dir" => {
                config.out_dir = if value == ".ai-runs" {
                    DEFAULT_ARTIFACT_DIR.to_string()
                } else {
                    value.to_string()
                };
            }
            "effort" => {
                if let Some(index) = EFFORTS.iter().position(|effort| *effort == value) {
                    config.effort_index = index;
                    legacy_effort = Some(index);
                }
            }
            "codex_effort" => {
                if let Some(index) = EFFORTS.iter().position(|effort| *effort == value) {
                    config.codex_effort_index = index;
                    codex_effort_seen = true;
                }
            }
            "claude_effort" => {
                if let Some(index) = EFFORTS.iter().position(|effort| *effort == value) {
                    config.claude_effort_index = index;
                    claude_effort_seen = true;
                }
            }
            "linked_effort" => {
                config.linked_effort_split = match value {
                    "split" | "per-model" | "true" => true,
                    "shared" | "common" | "false" => false,
                    _ => config.linked_effort_split,
                };
            }
            "split_effort" => {
                config.linked_effort_split = value == "true";
            }
            "effort_split" => {
                config.linked_effort_split = value == "true";
            }
            "linked_effort_split" => {
                config.linked_effort_split = value == "true";
            }
            "per_model_effort" => {
                config.linked_effort_split = value == "true";
            }
            "model_effort_mode" => {
                config.linked_effort_split = matches!(value, "split" | "per-model");
            }
            "effort_mode" => {
                config.linked_effort_split = matches!(value, "split" | "per-model");
            }
            "effort_per_model" => {
                config.linked_effort_split = value == "true";
            }
            "effort_shared" => {
                config.linked_effort_split = value != "true";
            }
            "effort_common" => {
                config.linked_effort_split = value != "true";
            }
            "common_effort" => {
                if let Some(index) = EFFORTS.iter().position(|effort| *effort == value) {
                    config.effort_index = index;
                    legacy_effort = Some(index);
                }
            }
            "last_chat" => {
                let chat_id = sanitize_chat_id(value);
                if !chat_id.is_empty() {
                    config.last_chat_id = Some(chat_id);
                }
            }
            "path_link_target" | "path_links" | "open_paths" => {
                config.path_link_target = PathTarget::from_config_str(value);
            }
            _ => {}
        }
    }

    if let Some(index) = legacy_effort {
        let effort = effort_label(index);
        if !codex_effort_seen && provider_supports_effort("codex", effort) {
            config.codex_effort_index = index;
        }
        if !claude_effort_seen && provider_supports_effort("claude", effort) {
            config.claude_effort_index = index;
        }
    }

    config
}

/// Атомарно записывает файл: пишет во временный файл рядом (в той же директории), синкает
/// на диск и переименовывает на место — rename атомарен, поэтому падение/kill в середине
/// записи не оставит усечённый config/history/чат (старый файл цел до последнего шага).
fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("clave");
    let tmp = path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()));
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)
}

/// Убирает символы, которые разбили бы построчный `key="value"`-формат конфига (переводы
/// строк). Обратно совместимо: обычные пути не меняются, а decode при чтении не нужен —
/// значит Windows-пути с обратным слэшем не искажаются.
fn config_value_sanitized(value: &str) -> String {
    value.replace(['\n', '\r'], " ")
}

pub(crate) fn save_config(path: &Path, config: &AppConfig) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = format!(
        concat!(
            "onboarding_done={}\n",
            "mode=\"{}\"\n",
            "direct_provider=\"{}\"\n",
            "theme=\"{}\"\n",
            "lang=\"{}\"\n",
            "rounds={}\n",
            "work_dir=\"{}\"\n",
            "out_dir=\"{}\"\n",
            "effort=\"{}\"\n",
            "codex_effort=\"{}\"\n",
            "claude_effort=\"{}\"\n",
            "linked_effort=\"{}\"\n",
            "last_chat=\"{}\"\n",
            "path_link_target=\"{}\"\n",
        ),
        config.onboarding_done,
        config.mode.as_str(),
        config.direct_provider.as_str(),
        config.theme.as_str(),
        config.lang.as_str(),
        config.rounds,
        config_value_sanitized(&config.work_dir),
        config_value_sanitized(&config.out_dir),
        effort_label(config.effort_index),
        effort_label(config.codex_effort_index),
        effort_label(config.claude_effort_index),
        if config.linked_effort_split {
            "split"
        } else {
            "shared"
        },
        config.last_chat_id.as_deref().unwrap_or(""),
        config
            .path_link_target
            .map(PathTarget::as_config_str)
            .unwrap_or(""),
    );
    write_atomic(path, content.as_bytes())
}

pub(crate) fn load_history(path: &Path) -> io::Result<Vec<String>> {
    let content = fs::read_to_string(path)?;
    let mut history = content
        .lines()
        .map(decode_field)
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();

    if history.len() > MAX_HISTORY_LINES {
        let remove_count = history.len() - MAX_HISTORY_LINES;
        history.drain(0..remove_count);
    }

    Ok(history)
}

pub(crate) fn save_history(path: &Path, history: &[String]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut out = String::new();
    for line in history
        .iter()
        .rev()
        .take(MAX_HISTORY_LINES)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        out.push_str(&encode_field(line));
        out.push('\n');
    }
    write_atomic(path, out.as_bytes())
}

#[derive(Clone)]
pub(crate) struct ChatSummary {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) lines: usize,
    pub(crate) modified: SystemTime,
}

pub(crate) fn restore_or_create_chat(
    chats_dir: &Path,
    last_chat_id: Option<&str>,
    lang: Language,
) -> (String, PathBuf, Vec<String>) {
    if let Some(id) = last_chat_id {
        let id = sanitize_chat_id(id);
        if !id.is_empty() {
            if let Some(path) = existing_chat_path(chats_dir, &id) {
                if let Ok(lines) = load_chat_transcript(&path) {
                    if !lines.is_empty() {
                        return (id, path, lines);
                    }
                }
            }
        }
    }

    let chat_id = new_chat_id();
    let path = chat_path_for_id(chats_dir, &chat_id);
    let transcript = initial_transcript(lang);
    (chat_id, path, transcript)
}

pub(crate) fn new_chat_id() -> String {
    format!("chat-{}", unix_millis())
}

pub(crate) fn chat_path_for_id(chats_dir: &Path, chat_id: &str) -> PathBuf {
    chats_dir.join(format!(
        "{}.{}",
        sanitize_chat_id(chat_id),
        CHAT_FILE_EXTENSION
    ))
}

/// Найти файл чата по id (расширение `.clave`).
pub(crate) fn existing_chat_path(chats_dir: &Path, chat_id: &str) -> Option<PathBuf> {
    let id = sanitize_chat_id(chat_id);
    if id.is_empty() {
        return None;
    }
    let path = chats_dir.join(format!("{id}.{CHAT_FILE_EXTENSION}"));
    path.exists().then_some(path)
}

pub(crate) fn sanitize_chat_id(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(&format!(".{}", CHAT_FILE_EXTENSION))
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect()
}

pub(crate) fn save_chat_transcript(
    path: &Path,
    chat_id: &str,
    transcript: &[String],
) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut out = String::new();
    out.push_str("# Clave Chat\n");
    out.push_str(&format!("id={chat_id}\n"));
    out.push_str(&format!("created={}\n", unix_millis()));
    out.push_str("---\n");
    for line in transcript {
        out.push_str(&format!("v1\t{}\n", encode_field(line)));
    }
    write_atomic(path, out.as_bytes())
}

pub(crate) fn append_chat_line(path: &Path, line: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    if !path.exists() {
        let chat_id = path
            .file_stem()
            .and_then(|value| value.to_str())
            .map(ToString::to_string)
            .unwrap_or_else(|| "unknown".to_string());
        save_chat_transcript(path, &chat_id, &[])?;
    }

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "v1\t{}", encode_field(line))
}

pub(crate) fn load_chat_transcript(path: &Path) -> io::Result<Vec<String>> {
    let content = fs::read_to_string(path)?;
    Ok(content
        .lines()
        .filter_map(|line| line.strip_prefix("v1\t"))
        .map(decode_field)
        .filter(|line| !is_welcome_line(line))
        .collect())
}

pub(crate) fn list_saved_chats(chats_dir: &Path, limit: usize) -> Vec<ChatSummary> {
    let Ok(entries) = fs::read_dir(chats_dir) else {
        return Vec::new();
    };

    let mut chats = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some(CHAT_FILE_EXTENSION))
        .filter_map(|path| chat_summary(&path))
        .collect::<Vec<_>>();

    chats.sort_by_key(|summary| std::cmp::Reverse(summary.modified));
    chats.truncate(limit);
    chats
}

pub(crate) fn chat_summary(path: &Path) -> Option<ChatSummary> {
    let id = path.file_stem()?.to_string_lossy().to_string();
    let modified = fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(UNIX_EPOCH);
    let lines = load_chat_transcript(path).ok()?;
    let title = chat_display_title(path, &lines, "empty chat");

    Some(ChatSummary {
        id,
        title,
        lines: lines.len(),
        modified,
    })
}

pub(crate) fn chat_display_title(path: &Path, lines: &[String], fallback: &str) -> String {
    read_chat_title(path)
        .map(|custom| truncate_chars(custom.trim(), 72))
        .or_else(|| first_prompt_title(lines))
        .or_else(|| {
            lines
                .iter()
                .find(|line| !line.trim().is_empty())
                .map(|line| truncate_chars(line.trim(), 72))
        })
        .unwrap_or_else(|| fallback.to_string())
}

/// Прочитать кастомный заголовок чата из header файла (строка `title=`).
pub(crate) fn read_chat_title(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    for line in content.lines() {
        if line == "---" {
            break;
        }
        if let Some(rest) = line.strip_prefix("title=") {
            let title = decode_field(rest);
            if !title.trim().is_empty() {
                return Some(title);
            }
        }
    }
    None
}

/// Записать кастомный заголовок чата в header (создаёт файл, если чата ещё нет).
pub(crate) fn set_chat_title(path: &Path, chat_id: &str, title: &str) -> io::Result<()> {
    if !path.exists() {
        save_chat_transcript(path, chat_id, &[])?;
    }
    let content = fs::read_to_string(path)?;
    let mut header = Vec::new();
    let mut body = Vec::new();
    let mut in_body = false;
    for line in content.lines() {
        if in_body {
            body.push(line);
        } else if line == "---" {
            in_body = true;
        } else if !line.starts_with("title=") {
            header.push(line);
        }
    }

    let mut out = String::new();
    for line in header {
        out.push_str(line);
        out.push('\n');
    }
    let trimmed = title.trim();
    if !trimmed.is_empty() {
        out.push_str(&format!("title={}\n", encode_field(trimmed)));
    }
    out.push_str("---\n");
    for line in body {
        out.push_str(line);
        out.push('\n');
    }
    write_atomic(path, out.as_bytes())
}

pub(crate) fn find_last_run(transcript: &[String]) -> Option<String> {
    transcript
        .iter()
        .rev()
        .find_map(|line| line.strip_prefix("Final brief: ").map(ToString::to_string))
}

pub(crate) fn encode_field(value: &str) -> String {
    let mut encoded = String::new();
    for ch in value.chars() {
        match ch {
            '\\' => encoded.push_str("\\\\"),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            '\t' => encoded.push_str("\\t"),
            _ => encoded.push(ch),
        }
    }
    encoded
}

pub(crate) fn decode_field(value: &str) -> String {
    let mut decoded = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            decoded.push(ch);
            continue;
        }

        match chars.next() {
            Some('n') => decoded.push('\n'),
            Some('r') => decoded.push('\r'),
            Some('t') => decoded.push('\t'),
            Some('\\') => decoded.push('\\'),
            Some(other) => decoded.push(other),
            None => decoded.push('\\'),
        }
    }
    decoded
}

pub(crate) fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_uses_existing_chat_then_falls_back() {
        let dir = env::temp_dir().join(format!("clave-restore-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");

        let id = "chat-restore-001";
        let path = chat_path_for_id(&dir, id);
        save_chat_transcript(&path, id, &["⏺ привет".to_string(), "ответ".to_string()])
            .expect("save chat");

        // last_chat_id с существующим непустым чатом → восстанавливаем его
        let (rid, _, lines) = restore_or_create_chat(&dir, Some(id), Language::Ru);
        assert_eq!(rid, id);
        assert_eq!(lines, vec!["⏺ привет".to_string(), "ответ".to_string()]);

        // None → создаём новый пустой
        let (nid, _, nlines) = restore_or_create_chat(&dir, None, Language::Ru);
        assert_ne!(nid, id);
        assert!(nlines.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn chat_title_round_trip_and_summary_priority() {
        let dir = env::temp_dir().join(format!("clave-title-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let id = "chat-title-1";
        let path = chat_path_for_id(&dir, id);
        save_chat_transcript(&path, id, &["◆ из контента".to_string()]).expect("save");

        // изначально заголовок берётся из контента
        assert_eq!(
            chat_summary(&path).map(|c| c.title),
            Some("из контента".to_string())
        );

        // задаём кастомный заголовок и читаем обратно
        set_chat_title(&path, id, "Мой чат").expect("set title");
        assert_eq!(read_chat_title(&path), Some("Мой чат".to_string()));

        // chat_summary теперь отдаёт приоритет кастомному заголовку
        assert_eq!(
            chat_summary(&path).map(|c| c.title),
            Some("Мой чат".to_string())
        );
        // тело чата не пострадало
        assert_eq!(
            load_chat_transcript(&path).unwrap(),
            vec!["◆ из контента".to_string()]
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn chat_display_title_prefers_first_prompt_before_generic_lines() {
        let dir = env::temp_dir().join(format!("clave-title-fallback-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let id = "chat-title-fallback";
        let path = chat_path_for_id(&dir, id);
        let lines = vec![
            "✦ clave готов".to_string(),
            "◆ Первый реальный промт".to_string(),
        ];
        save_chat_transcript(&path, id, &lines).expect("save");

        assert_eq!(
            chat_display_title(&path, &lines, id),
            "Первый реальный промт"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_atomic_replaces_and_leaves_no_tmp() {
        let dir = env::temp_dir().join(format!("clave-atomic-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("data");

        write_atomic(&path, b"first").expect("write 1");
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");
        // Перезапись существующего файла заменяет содержимое и не оставляет мусора.
        write_atomic(&path, b"second").expect("write 2");
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");

        let tmp_left = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().contains(".tmp"));
        assert!(!tmp_left, "временный файл не был убран");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn config_value_sanitized_strips_newlines_keeps_backslashes() {
        // Windows-путь с обратным слэшем не искажается (иначе сломался бы round-trip).
        assert_eq!(
            config_value_sanitized(r"C:\Users\me\proj"),
            r"C:\Users\me\proj"
        );
        // Переводы строк, которые разбили бы формат key="value", убираются.
        assert_eq!(config_value_sanitized("a\nb\rc"), "a b c");
    }
}
