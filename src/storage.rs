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

/// Папка чатов ДЛЯ КОНКРЕТНОЙ рабочей директории: `~/.clave/chats/<ключ>`. Чаты изолированы
/// по каталогу запуска — открыв clave в другом проекте, пользователь видит его чаты, а не
/// чужие. Раньше пул был общим на все каталоги, поэтому `/chats` и авто-восстановление при
/// старте подсовывали чат из совсем другого проекта.
pub(crate) fn chats_dir_for(work_dir: &Path) -> PathBuf {
    chats_dir().join(dir_key(work_dir))
}

/// Имя папки для каталога: читаемый базовый компонент + стабильный хэш ПОЛНОГО канонического
/// пути. Базовое имя — чтобы папку можно было опознать глазами; хэш — чтобы два одноимённых
/// каталога в разных местах (`~/work/api` и `~/tmp/api`) не делили чаты.
fn dir_key(work_dir: &Path) -> String {
    let canonical = work_dir
        .canonicalize()
        .unwrap_or_else(|_| work_dir.to_path_buf());
    let base: String = canonical
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("root")
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .take(32)
        .collect();
    let base = if base.is_empty() {
        "root".to_string()
    } else {
        base
    };
    format!(
        "{base}-{}",
        stable_hash_hex(canonical.to_string_lossy().as_bytes())
    )
}

/// FNV-1a (64-бит) → 8 hex. Свой хэш, а НЕ `DefaultHasher`: его результат не гарантирован между
/// версиями Rust, и апгрейд тулчейна «потерял» бы папку чатов каталога (ключ бы поехал).
fn stable_hash_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:08x}", (hash & 0xffff_ffff) as u32)
}

pub(crate) fn config_path() -> PathBuf {
    if let Ok(path) = env::var("CLAVE_CONFIG") {
        return PathBuf::from(path);
    }

    clave_state_dir().join("config")
}

pub(crate) fn load_config(path: &Path) -> AppConfig {
    // Читаем БАЙТАМИ и декодируем лоссово: один битый UTF-8 байт не должен обнулять
    // весь конфиг (read_to_string упал бы целиком → старт на дефолтах, а следующий save
    // затёр бы существующий файл). Так уцелеют все валидные строки key=value.
    let Ok(bytes) = fs::read(path) else {
        return AppConfig::default();
    };
    let content = String::from_utf8_lossy(&bytes);

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
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    // При сбое записи/синка/переименования не оставляем осиротевший .tmp в каталоге
    // состояния (иначе они копятся). На успехе tmp уже переименован — удалять нечего.
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
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
    // 1. Точный resume по last_chat_id — если этот чат лежит В ЭТОЙ папке и непустой.
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

    // 2. Иначе — самый свежий непустой чат ЭТОЙ папки. Папка per-directory (чужие сюда не
    //    попадают), а глобальный `last_chat_id` мог указывать на чат другого каталога — без
    //    этого fallback возврат в проект открывал бы пустой чат вместо последнего.
    for summary in list_saved_chats(chats_dir, usize::MAX) {
        if let Some(path) = existing_chat_path(chats_dir, &summary.id) {
            if let Ok(lines) = load_chat_transcript(&path) {
                if !lines.is_empty() {
                    return (summary.id, path, lines);
                }
            }
        }
    }

    // 3. Пусто — новый чат.
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

        // None, но в ПАПКЕ есть непустой чат → возвращаемся в него (per-directory resume),
        // а не открываем пустой: вернувшись в проект, пользователь продолжает последний чат.
        // Глобальный last_chat_id мог указывать на чат другого каталога — тогда и срабатывает.
        let (nid, _, nlines) = restore_or_create_chat(&dir, None, Language::Ru);
        assert_eq!(nid, id, "fallback — последний чат ЭТОЙ папки");
        assert!(!nlines.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_creates_new_chat_when_folder_is_empty() {
        let dir = env::temp_dir().join(format!("clave-restore-empty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");

        // Пустая папка (новый каталог) → новый пустой чат, ничего чужого не подтягивается.
        let (id, _, lines) = restore_or_create_chat(&dir, None, Language::Ru);
        assert!(id.starts_with("chat-"));
        assert!(lines.is_empty(), "пустая папка каталога → новый пустой чат");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn chats_dir_is_isolated_per_working_directory() {
        // Суть фикса: разные рабочие каталоги → разные папки чатов; один и тот же каталог →
        // стабильно та же папка (иначе чат «терялся» бы между запусками в одном проекте).
        let a = env::temp_dir().join(format!("clave-scope-a-{}", std::process::id()));
        let b = env::temp_dir().join(format!("clave-scope-b-{}", std::process::id()));
        let _ = fs::create_dir_all(&a);
        let _ = fs::create_dir_all(&b);

        let dir_a = chats_dir_for(&a);
        let dir_b = chats_dir_for(&b);
        assert_ne!(dir_a, dir_b, "у разных каталогов — разные папки чатов");
        assert_eq!(
            dir_a,
            chats_dir_for(&a),
            "тот же каталог — та же папка (ключ стабилен)"
        );
        assert!(
            dir_a.starts_with(chats_dir()),
            "папка каталога лежит под общим корнем ~/.clave/chats"
        );

        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
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

    /// Изолированный каталог под тест: только temp_dir, никаких путей в $HOME.
    fn temp_case(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("clave-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    /// Записать мини-конфиг и сразу прочитать его продуктовым парсером.
    fn config_from(dir: &Path, name: &str, content: &str) -> AppConfig {
        let path = dir.join(name);
        fs::write(&path, content).expect("write config");
        load_config(&path)
    }

    #[test]
    fn config_survives_an_invalid_utf8_byte() {
        let dir = temp_case("config-bad-utf8");
        let path = dir.join("config");
        let saved = AppConfig {
            rounds: 7,
            lang: Language::En,
            ..AppConfig::default()
        };
        save_config(&path, &saved).expect("save");

        // Вставляем битую UTF-8 строку в середину: реальные строки key=value уцелеют,
        // битая пропустится (нет '='). Раньше read_to_string падал на 0xFF целиком → ВСЕ
        // дефолты, и следующий save затёр бы существующий файл.
        let mut bytes = fs::read(&path).expect("read");
        if let Some(nl) = bytes.iter().position(|&b| b == b'\n') {
            bytes.splice(nl + 1..nl + 1, [0xFF, b'\n']);
        }
        fs::write(&path, &bytes).expect("rewrite");

        let loaded = load_config(&path);
        assert_eq!(loaded.rounds, 7, "rounds уцелел, несмотря на битый байт");
        assert_eq!(loaded.lang, Language::En, "lang уцелел");
    }

    #[test]
    fn write_atomic_removes_the_tmp_file_when_the_write_fails() {
        let dir = temp_case("atomic-fail");
        // Пишем «в» существующий подкаталог: rename файла НА каталог падает.
        let target = dir.join("subdir");
        fs::create_dir_all(&target).expect("mkdir");

        let result = write_atomic(&target, b"data");

        assert!(result.is_err(), "rename файла на каталог обязан упасть");
        let tmp = dir.join(format!(".subdir.{}.tmp", std::process::id()));
        assert!(!tmp.exists(), "осиротевший .tmp убран после сбоя записи");
    }

    #[test]
    fn config_round_trip_preserves_every_field() {
        let dir = temp_case("config-roundtrip");
        let path = dir.join("config");

        let saved = AppConfig {
            onboarding_done: true,
            mode: Mode::CodexClaude,
            direct_provider: Provider::Claude,
            theme: Theme::Amber,
            lang: Language::En,
            rounds: 5,
            work_dir: "/tmp/work".to_string(),
            out_dir: "artifacts".to_string(),
            effort_index: 2,
            codex_effort_index: 0,
            claude_effort_index: 1,
            linked_effort_split: false,
            last_chat_id: Some("chat-42".to_string()),
            path_link_target: Some(PathTarget::Cursor),
        };
        save_config(&path, &saved).expect("save config");

        // Каждое поле отличается от дефолта — значит сверка ловит и «ничего не записали»,
        // и «ничего не прочитали».
        let loaded = load_config(&path);
        assert!(loaded.onboarding_done);
        assert_eq!(loaded.mode, Mode::CodexClaude);
        assert_eq!(loaded.direct_provider, Provider::Claude);
        assert_eq!(loaded.theme, Theme::Amber);
        assert_eq!(loaded.lang, Language::En);
        assert_eq!(loaded.rounds, 5);
        assert_eq!(loaded.work_dir, "/tmp/work");
        assert_eq!(loaded.out_dir, "artifacts");
        assert_eq!(loaded.effort_index, 2);
        assert_eq!(loaded.codex_effort_index, 0);
        assert_eq!(loaded.claude_effort_index, 1);
        assert!(!loaded.linked_effort_split);
        assert_eq!(loaded.last_chat_id.as_deref(), Some("chat-42"));
        assert_eq!(loaded.path_link_target, Some(PathTarget::Cursor));

        // Несуществующий файл → чистые дефолты.
        let missing = load_config(&dir.join("nope"));
        assert!(!missing.onboarding_done);
        assert_eq!(missing.rounds, AppConfig::default().rounds);
        assert_eq!(missing.last_chat_id, None);

        // Перевод строки в значении не должен разваливать построчный формат.
        let with_newline = AppConfig {
            work_dir: "a\nb".to_string(),
            ..saved
        };
        save_config(&path, &with_newline).expect("save config 2");
        let loaded = load_config(&path);
        assert_eq!(loaded.work_dir, "a b");
        assert_eq!(loaded.theme, Theme::Amber);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn out_dir_legacy_value_is_remapped() {
        let dir = temp_case("config-outdir");
        assert_eq!(
            config_from(&dir, "legacy", "out_dir=\".ai-runs\"\n").out_dir,
            DEFAULT_ARTIFACT_DIR
        );
        assert_eq!(
            config_from(&dir, "custom", "out_dir=\"builds\"\n").out_dir,
            "builds"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn last_chat_is_sanitized_and_empty_is_ignored() {
        let dir = temp_case("config-lastchat");
        assert_eq!(
            config_from(&dir, "traversal", "last_chat=\"../../etc\"\n").last_chat_id,
            Some("etc".to_string())
        );
        // После вычистки мусорных символов не осталось ничего → чата нет.
        assert_eq!(
            config_from(&dir, "garbage", "last_chat=\"///\"\n").last_chat_id,
            None
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_effort_fills_only_supported_providers() {
        let dir = temp_case("config-legacy-effort");

        // "low" поддерживают оба провайдера → раздаётся обоим.
        let low = config_from(&dir, "low", "effort=\"low\"\n");
        assert_eq!(low.effort_index, 0);
        assert_eq!(low.codex_effort_index, 0);
        assert_eq!(low.claude_effort_index, 0);

        // "max" есть только у claude → codex остаётся на своём дефолте.
        let max = config_from(&dir, "max", "effort=\"max\"\n");
        assert_eq!(max.effort_index, 4);
        assert_eq!(
            max.codex_effort_index,
            AppConfig::default().codex_effort_index
        );
        assert_eq!(max.claude_effort_index, 4);

        // "xhigh" есть только у codex → claude остаётся на своём дефолте.
        let xhigh = config_from(&dir, "xhigh", "effort=\"xhigh\"\n");
        assert_eq!(xhigh.codex_effort_index, 3);
        assert_eq!(
            xhigh.claude_effort_index,
            AppConfig::default().claude_effort_index
        );

        // Явные per-provider значения легаси-ключ не перетирает.
        let explicit = config_from(
            &dir,
            "explicit",
            "effort=\"low\"\ncodex_effort=\"high\"\nclaude_effort=\"medium\"\n",
        );
        assert_eq!(explicit.effort_index, 0);
        assert_eq!(explicit.codex_effort_index, 2);
        assert_eq!(explicit.claude_effort_index, 1);

        // common_effort — алиас legacy-effort со всей раздачей.
        let common = config_from(&dir, "common", "common_effort=\"low\"\n");
        assert_eq!(common.effort_index, 0);
        assert_eq!(common.codex_effort_index, 0);
        assert_eq!(common.claude_effort_index, 0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn effort_split_aliases_all_flip_the_flag() {
        let dir = temp_case("config-split-alias");
        assert!(
            AppConfig::default().linked_effort_split,
            "тест опирается на дефолт split=true"
        );

        // Все алиасы «true/false» должны увести флаг из дефолтного true в false.
        for key in [
            "split_effort",
            "effort_split",
            "linked_effort_split",
            "per_model_effort",
            "effort_per_model",
        ] {
            let config = config_from(&dir, key, &format!("{key}=\"false\"\n"));
            assert!(!config.linked_effort_split, "ключ {key} не выключил split");
        }

        // Инвертированные алиасы: shared=true означает split=false.
        for key in ["effort_shared", "effort_common"] {
            let config = config_from(&dir, key, &format!("{key}=\"true\"\n"));
            assert!(!config.linked_effort_split, "ключ {key} не выключил split");
        }

        // Режимные алиасы: стартуем из false, чтобы «split» был реальным изменением.
        for key in ["model_effort_mode", "effort_mode"] {
            let config = config_from(
                &dir,
                key,
                &format!("split_effort=\"false\"\n{key}=\"split\"\n"),
            );
            assert!(config.linked_effort_split, "ключ {key} не включил split");
        }

        // linked_effort проверяем в обе стороны, каждый раз стартуя из противоположного.
        assert!(
            !config_from(&dir, "linked-shared", "linked_effort=\"shared\"\n").linked_effort_split
        );
        assert!(
            config_from(
                &dir,
                "linked-split",
                "split_effort=\"false\"\nlinked_effort=\"per-model\"\n"
            )
            .linked_effort_split
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_round_trip_and_cap() {
        let dir = temp_case("history");
        let path = dir.join("history");

        // Сохранение обрезает историю до последних MAX_HISTORY_LINES.
        let history = (0..250).map(|i| format!("line-{i}")).collect::<Vec<_>>();
        save_history(&path, &history).expect("save history");
        let loaded = load_history(&path).expect("load history");
        assert_eq!(loaded.len(), MAX_HISTORY_LINES);
        assert_eq!(loaded.first().unwrap(), "line-50");
        assert_eq!(loaded.last().unwrap(), "line-249");

        // Многострочная команда переживает round-trip (encode/decode полей).
        save_history(&path, &["echo a\nb\tc".to_string()]).expect("save history 2");
        assert_eq!(
            load_history(&path).unwrap(),
            vec!["echo a\nb\tc".to_string()]
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_history_caps_oversized_file_and_drops_blanks() {
        let dir = temp_case("history-oversized");
        let path = dir.join("history");

        // Файл заведомо длиннее лимита (мог остаться от старой версии) — чтение обязано
        // отдать ровно последние MAX_HISTORY_LINES строк, пустые не в счёт.
        let mut raw = String::from("\n   \n");
        for i in 0..250 {
            raw.push_str(&format!("line-{i}\n"));
        }
        fs::write(&path, raw).expect("write history");

        let loaded = load_history(&path).expect("load history");
        assert_eq!(loaded.len(), MAX_HISTORY_LINES);
        assert_eq!(loaded.first().unwrap(), "line-50");
        assert_eq!(loaded.last().unwrap(), "line-249");
        assert!(!loaded.iter().any(|line| line.trim().is_empty()));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_chat_line_creates_file_and_keeps_previous_lines() {
        let dir = temp_case("append-chat");
        let path = chat_path_for_id(&dir, "chat-append");

        // Файла ещё нет → append обязан создать чат с header и записать строку.
        append_chat_line(&path, "первая").expect("append 1");
        let raw = fs::read_to_string(&path).expect("read chat");
        assert!(raw.starts_with("# Clave Chat\n"), "нет header: {raw}");
        assert_eq!(
            load_chat_transcript(&path).unwrap(),
            vec!["первая".to_string()]
        );

        // Файл уже есть → append дописывает, а не затирает.
        append_chat_line(&path, "вторая").expect("append 2");
        assert_eq!(
            load_chat_transcript(&path).unwrap(),
            vec!["первая".to_string(), "вторая".to_string()]
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_saved_chats_returns_only_chat_files() {
        let dir = temp_case("list-chats");
        for id in ["chat-a", "chat-b", "chat-c"] {
            save_chat_transcript(&chat_path_for_id(&dir, id), id, &[format!("◆ {id}")])
                .expect("save chat");
        }
        // Посторонний файл в том же каталоге не должен попасть в список.
        fs::write(dir.join("notes.txt"), "не чат").expect("write notes");

        let listed = list_saved_chats(&dir, 10);
        let mut ids = listed.iter().map(|c| c.id.clone()).collect::<Vec<_>>();
        ids.sort();
        assert_eq!(ids, vec!["chat-a", "chat-b", "chat-c"]);
        assert!(listed.iter().all(|c| c.lines == 1));

        // limit реально ограничивает выдачу.
        assert_eq!(list_saved_chats(&dir, 2).len(), 2);
        // Несуществующий каталог → пусто.
        assert!(list_saved_chats(&dir.join("nope"), 10).is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_chat_title_replaces_previous_title() {
        let dir = temp_case("chat-retitle");
        let id = "chat-retitle";
        let path = chat_path_for_id(&dir, id);
        save_chat_transcript(&path, id, &["◆ тело чата".to_string()]).expect("save");

        set_chat_title(&path, id, "Первый").expect("title 1");
        set_chat_title(&path, id, "Второй").expect("title 2");

        assert_eq!(read_chat_title(&path), Some("Второй".to_string()));
        let raw = fs::read_to_string(&path).expect("read chat");
        // Старый заголовок вычищен из header, header остался header'ом, тело цело.
        assert_eq!(raw.matches("title=").count(), 1);
        assert!(raw.starts_with("# Clave Chat\n"), "header уехал: {raw}");
        assert_eq!(
            load_chat_transcript(&path).unwrap(),
            vec!["◆ тело чата".to_string()]
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn chat_display_title_falls_back_to_first_nonblank_line() {
        let dir = temp_case("chat-title-blank");
        // Файла нет → кастомного заголовка нет; в строках нет ни одного «◆».
        let path = chat_path_for_id(&dir, "chat-none");
        let lines = vec!["   ".to_string(), "просто текст".to_string()];
        assert_eq!(chat_display_title(&path, &lines, "empty"), "просто текст");
        assert_eq!(chat_display_title(&path, &[], "empty"), "empty");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn state_paths_are_derived_from_state_dir() {
        // Окружение не трогаем (тесты идут параллельно) — проверяем согласованность путей.
        let root = clave_state_dir();
        assert!(!root.as_os_str().is_empty());
        assert_eq!(history_path(), root.join("history"));
        assert_eq!(chats_dir(), root.join("chats"));

        let expected_config = match env::var("CLAVE_CONFIG") {
            Ok(path) => PathBuf::from(path),
            Err(_) => root.join("config"),
        };
        assert_eq!(config_path(), expected_config);

        match env::var("HOME") {
            Ok(home) => assert_eq!(
                default_home_state_dir(".clave-test"),
                Some(PathBuf::from(home).join(".clave-test"))
            ),
            Err(_) => assert_eq!(default_home_state_dir(".clave-test"), None),
        }
    }

    #[test]
    fn find_last_run_takes_the_latest_brief() {
        let transcript = vec![
            "Final brief: /tmp/first".to_string(),
            "какой-то ответ".to_string(),
            "Final brief: /tmp/second".to_string(),
        ];
        assert_eq!(find_last_run(&transcript), Some("/tmp/second".to_string()));
        assert_eq!(find_last_run(&["просто строка".to_string()]), None);
        assert_eq!(find_last_run(&[]), None);
    }

    #[test]
    fn new_chat_id_carries_a_real_timestamp() {
        // Сентябрь 2020 — заведомо в прошлом; ноль/единица из мутанта сюда не попадут.
        const PAST: u128 = 1_600_000_000_000;
        assert!(unix_millis() > PAST);

        let id = new_chat_id();
        let millis = id
            .strip_prefix("chat-")
            .expect("id начинается с chat-")
            .parse::<u128>()
            .expect("хвост id — это миллисекунды");
        assert!(millis > PAST);
    }

    #[test]
    fn is_welcome_line_matches_every_greeting() {
        for line in [
            "✦ Добро пожаловать в clave",
            "✦ Welcome to clave",
            "Введите задачу и нажмите Enter",
            "Type a task and press Enter",
            "Это Claude Code-style интерфейс",
            "This is a Claude Code-style UI",
            "  ✦ Welcome to clave  ",
        ] {
            assert!(is_welcome_line(line), "не распознано приветствие: {line}");
        }
        assert!(!is_welcome_line("◆ обычный промт"));
        assert!(!is_welcome_line(""));
    }

    #[test]
    fn truncate_chars_handles_zero_and_one() {
        assert_eq!(truncate_chars("привет", 0), "");
        assert_eq!(truncate_chars("привет", 1), "…");
        assert_eq!(truncate_chars("привет", 3), "пр…");
        assert_eq!(truncate_chars("привет", 6), "привет");
    }

    #[test]
    fn sanitize_chat_id_keeps_only_id_chars() {
        assert_eq!(sanitize_chat_id("chat-7_a /..x!"), "chat-7_ax");
        assert_eq!(sanitize_chat_id("chat-9.clave"), "chat-9");
        assert_eq!(sanitize_chat_id("../../etc"), "etc");
        assert_eq!(sanitize_chat_id("///"), "");
    }

    #[test]
    fn encode_field_escapes_control_chars() {
        // Через файл \r и \t не отличить от сырых символов — проверяем кодировщик напрямую.
        assert_eq!(encode_field("a\\b\nc\rd\te"), "a\\\\b\\nc\\rd\\te");
        assert_eq!(decode_field("a\\\\b\\nc\\rd\\te"), "a\\b\nc\rd\te");
        assert_eq!(encode_field("обычная строка"), "обычная строка");
    }

    #[test]
    fn final_brief_extracts_known_sections() {
        let dir = temp_case("brief-sections");
        let path = dir.join("brief.md");
        fs::write(
            &path,
            "шапка\n## Header\nмусор\n## Current Spec\nспека 1\n## Last Review\nревью 1\n",
        )
        .expect("write brief");

        let lines = final_brief_lines_for_chat(path.to_str().unwrap(), Language::Ru).unwrap();
        assert_eq!(
            lines,
            vec![
                "## Текущая спека".to_string(),
                "спека 1".to_string(),
                "## Последнее ревью".to_string(),
                "ревью 1".to_string(),
            ]
        );

        // Английская локаль подставляет свои заголовки.
        let en = final_brief_lines_for_chat(path.to_str().unwrap(), Language::En).unwrap();
        assert_eq!(en.first().unwrap(), "## Current Spec");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn final_brief_falls_back_to_whole_file() {
        let dir = temp_case("brief-fallback");

        // Секций нет вовсе → отдаём файл целиком.
        let plain = dir.join("plain.md");
        fs::write(&plain, "привет\nмир\n").expect("write");
        assert_eq!(
            final_brief_lines_for_chat(plain.to_str().unwrap(), Language::Ru).unwrap(),
            vec!["привет".to_string(), "мир".to_string()]
        );

        // Секция есть, но пустая → одни заголовки бесполезны, отдаём файл целиком.
        let empty = dir.join("empty.md");
        fs::write(&empty, "заметка\n## Current Spec\n").expect("write");
        assert_eq!(
            final_brief_lines_for_chat(empty.to_str().unwrap(), Language::Ru).unwrap(),
            vec!["заметка".to_string(), "## Current Spec".to_string()]
        );

        // Несуществующий файл → ошибка, а не пустой список.
        assert!(
            final_brief_lines_for_chat(dir.join("nope.md").to_str().unwrap(), Language::Ru)
                .is_err()
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn final_brief_collapses_blanks_and_truncates() {
        let dir = temp_case("brief-compact");

        // Подряд идущие пустые строки схлопываются в одну.
        let blanks = dir.join("blanks.md");
        fs::write(&blanks, "a\n\n\nb\n").expect("write");
        assert_eq!(
            final_brief_lines_for_chat(blanks.to_str().unwrap(), Language::Ru).unwrap(),
            vec!["a".to_string(), String::new(), "b".to_string()]
        );

        // Длинный файл обрезается на 140 строках + маркер обрезки.
        let long = dir.join("long.md");
        let content = (0..200)
            .map(|i| format!("line-{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&long, content).expect("write");
        let lines = final_brief_lines_for_chat(long.to_str().unwrap(), Language::Ru).unwrap();
        assert_eq!(lines.len(), 141);
        assert_eq!(lines[139], "line-139");
        assert!(lines[140].starts_with("… ответ обрезан"));

        // Слишком длинная строка режется по символам.
        let wide = dir.join("wide.md");
        fs::write(&wide, "я".repeat(300)).expect("write");
        let wide_lines = final_brief_lines_for_chat(wide.to_str().unwrap(), Language::Ru).unwrap();
        assert_eq!(wide_lines[0].chars().count(), 220);
        assert!(wide_lines[0].ends_with('…'));

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
