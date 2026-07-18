use super::*;

/// Приветственный блок (Claude-style): логотип слева + имя/модель/cwd справа, без
/// рамок, и строка-подсказка. Кладётся в ленту при пустом старте и после `/clear`,
/// уходит в скроллбэк по мере диалога. Строки помечены PUA-сентинелами
/// (`WELCOME_*`), стилизуются в `style_transcript_line`.
pub(crate) fn welcome_lines(app: &App) -> Vec<String> {
    let lang = app.lang;
    let version = env!("CARGO_PKG_VERSION");
    let cwd = abbreviate_home(&app.resolved_work_dir());
    let model = format!(
        "{} · chat {} · effort {}",
        app.mode.as_str(),
        app.direct_provider.as_str(),
        app.effort_summary()
    );
    // Робот clave (нарисован пользователем, 16×16 → Unicode-полублоки), красится
    // акцентом темы. Все строки одной ширины — чтобы инфо справа выровнялось.
    let logo = [
        "  ▄████████▄  ",
        "  ██████████  ",
        "▀████████████▀",
        "  ▄▄▄▄▄▄▄▄▄▄  ",
        "  ███▀  ▀███  ",
        "      ▄▄      ",
        "    █▀  ▀█    ",
        "    ▀▄██▄▀    ",
    ];
    let hint = lang.choose(
        "Пиши сообщение — прямой чат · /plan — спека · /help — все команды",
        "Type a message — direct chat · /plan — spec · /help — all commands",
    );
    vec![
        // Инфо — вверху, у головы робота (строки 0-2); ниже — только логотип.
        format!(
            "{WELCOME_NAME}{}{WELCOME_SEP}clave{WELCOME_SEP}v{version}",
            logo[0]
        ),
        format!("{WELCOME_INFO}{}{WELCOME_SEP}{model}", logo[1]),
        format!("{WELCOME_INFO}{}{WELCOME_SEP}{cwd}", logo[2]),
        format!("{WELCOME_INFO}{}", logo[3]),
        format!("{WELCOME_INFO}{}", logo[4]),
        format!("{WELCOME_INFO}{}", logo[5]),
        format!("{WELCOME_INFO}{}", logo[6]),
        format!("{WELCOME_INFO}{}", logo[7]),
        String::new(),
        format!("{WELCOME_HINT}{hint}"),
    ]
}

/// Сокращает `$HOME` до `~` в начале пути (как cwd в welcome у Claude).
fn abbreviate_home(path: &Path) -> String {
    abbreviate_with_home(path, std::env::var("HOME").ok().as_deref())
}

/// То же, но `$HOME` — ПАРАМЕТР. Шов ради тестов: иначе проверить пустой и отсутствующий HOME
/// можно было бы только правкой глобальной переменной окружения — то есть гонкой со всеми
/// соседними тестами, которые её читают.
///
/// Пустой HOME обязан отсекаться: без этого `strip_prefix("")` срабатывает на ЛЮБОМ пути, и
/// каждый путь превратился бы в «~/…» — включая `/usr/local/bin`.
fn abbreviate_with_home(path: &Path, home: Option<&str>) -> String {
    let shown = path.display().to_string();
    match home {
        Some(home) if !home.is_empty() => match shown.strip_prefix(home) {
            // «~» только если HOME — ЦЕЛЫЙ компонент пути: дальше либо конец, либо '/'.
            // Иначе при HOME=/home/bob путь /home/bobby/project стал бы «~by/project».
            Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("~{rest}"),
            _ => shown,
        },
        _ => shown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abbreviate_home_shortens_only_a_real_home() {
        let path = Path::new("/Users/кто-то/проект/файл.rs");

        assert_eq!(
            abbreviate_with_home(path, Some("/Users/кто-то")),
            "~/проект/файл.rs",
            "домашний каталог обязан сократиться до тильды"
        );
        // Чужой путь не трогаем.
        assert_eq!(
            abbreviate_with_home(Path::new("/usr/local/bin"), Some("/Users/кто-то")),
            "/usr/local/bin"
        );
        // Совпадение по СТРОКЕ, но не по компоненту пути: чужой каталог с общим префиксом
        // не смеет маскироваться под домашний (HOME=/Users/кто-то, путь /Users/кто-тоXX/…).
        assert_eq!(
            abbreviate_with_home(Path::new("/Users/кто-тоXX/проект"), Some("/Users/кто-то")),
            "/Users/кто-тоXX/проект",
            "префикс без границы компонента не даёт тильду"
        );
        // Сам HOME без хвоста → просто тильда.
        assert_eq!(
            abbreviate_with_home(Path::new("/Users/кто-то"), Some("/Users/кто-то")),
            "~"
        );
        // ПУСТОЙ HOME — ловушка: `strip_prefix("")` срабатывает на любом пути, и без отсечки
        // каждый путь превратился бы в «~/…», включая системные.
        assert_eq!(
            abbreviate_with_home(Path::new("/usr/local/bin"), Some("")),
            "/usr/local/bin",
            "пустой HOME не смеет сокращать вообще всё"
        );
        assert_eq!(
            abbreviate_with_home(Path::new("/usr/local/bin"), None),
            "/usr/local/bin",
            "без HOME путь показывается как есть"
        );
    }

    #[test]
    fn abbreviate_home_reads_the_real_home() {
        // Обёртка обязана и правда лезть в окружение, а не возвращать константу.
        let home = std::env::var("HOME").expect("HOME задан в любом окружении");
        let path = Path::new(&home).join("клаве-проект");
        assert_eq!(abbreviate_home(&path), "~/клаве-проект");
    }
}
