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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::testkit::*;

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
    fn sanitize_chat_id_keeps_only_id_chars() {
        assert_eq!(sanitize_chat_id("chat-7_a /..x!"), "chat-7_ax");
        assert_eq!(sanitize_chat_id("chat-9.clave"), "chat-9");
        assert_eq!(sanitize_chat_id("../../etc"), "etc");
        assert_eq!(sanitize_chat_id("///"), "");
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
}
