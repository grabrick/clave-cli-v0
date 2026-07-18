use crate::prelude::*;
use crate::*;

mod chats;
mod config;
pub(crate) use chats::*;
pub(crate) use config::*;

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
pub(crate) mod testkit {
    use super::*;

    /// Изолированный каталог под тест: только temp_dir, никаких путей в $HOME.
    pub(crate) fn temp_case(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("clave-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn encode_field_escapes_control_chars() {
        // Через файл \r и \t не отличить от сырых символов — проверяем кодировщик напрямую.
        assert_eq!(encode_field("a\\b\nc\rd\te"), "a\\\\b\\nc\\rd\\te");
        assert_eq!(decode_field("a\\\\b\\nc\\rd\\te"), "a\\b\nc\rd\te");
        assert_eq!(encode_field("обычная строка"), "обычная строка");
    }
}
