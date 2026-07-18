use crate::prelude::*;
use crate::*;

pub(crate) fn engine_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("CLAVE_ENGINE") {
        if let Some(path) = existing_path(PathBuf::from(path)) {
            return Some(path);
        }
    }

    if let Ok(current_dir) = env::current_dir() {
        if let Some(path) = existing_path(current_dir.join(ENGINE_NAME)) {
            return Some(path);
        }
    }

    if let Ok(exe) = env::current_exe() {
        for dir in exe.ancestors().skip(1).take(4) {
            if let Some(path) = existing_path(dir.join(ENGINE_NAME)) {
                return Some(path);
            }
        }
    }

    // Последний фолбэк: движок вшит в бинарник. Установленный через `cargo install`
    // `clave` живёт один (без скриптов рядом) — распаковываем встроенную копию в
    // кэш состояния и работаем с ней. В dev-чекауте сюда не доходим: скрипты
    // находятся в cwd/рядом с exe выше, и правки видны сразу.
    embedded_engine_path()
}

/// Движок, вшитый на этапе компиляции (путь — от src/ к корню репозитория).
const EMBEDDED_SPEC_CLAVE: &str = include_str!("../../spec-clave");

/// Путь к распакованной встроенной копии движка (`spec-clave`).
fn embedded_engine_path() -> Option<PathBuf> {
    extract_engine_to(&clave_state_dir().join("engine"))
}

/// Распаковывает вшитый движок в `dir` (идемпотентно, по «штампу» содержимого) и
/// возвращает путь к `spec-clave`.
fn extract_engine_to(dir: &Path) -> Option<PathBuf> {
    let engine = dir.join(ENGINE_NAME);
    let stamp_path = dir.join(".stamp");
    let want = engine_stamp();

    // Перезаписываем только если содержимое сменилось (обновление бинарника) или
    // файла нет — иначе не трогаем диск на каждом запуске плана.
    let fresh = engine.exists() && fs::read_to_string(&stamp_path).is_ok_and(|s| s.trim() == want);
    if !fresh {
        fs::create_dir_all(dir).ok()?;
        write_engine_file(&engine, EMBEDDED_SPEC_CLAVE)?;
        let _ = fs::write(&stamp_path, &want);
    }
    existing_path(engine)
}

/// Записывает файл движка и на unix ставит исполняемый бит (shebang сам по себе не
/// делает файл исполняемым). На Windows бит не нужен — `/plan` там идёт через bash
/// (WSL/Git Bash), а сам файл всё равно читается интерпретатором.
fn write_engine_file(path: &Path, content: &str) -> Option<()> {
    fs::write(path, content).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o755));
    }
    Some(())
}

/// Короткий «штамп» содержимого движка (FNV-1a, без внешних зависимостей):
/// меняется при правке движка → распаковка обновит файл.
fn engine_stamp() -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in EMBEDDED_SPEC_CLAVE.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

pub(crate) fn existing_path(path: PathBuf) -> Option<PathBuf> {
    if !path.exists() {
        return None;
    }
    Some(path.canonicalize().unwrap_or(path))
}

pub(crate) fn launch_work_dir() -> PathBuf {
    env::var("CLAVE_LAUNCH_CWD")
        .ok()
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .and_then(existing_path)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub(crate) fn resolve_work_dir(configured: &str, base_dir: &Path) -> PathBuf {
    let configured = configured.trim();
    if configured.is_empty() || configured == "." {
        return base_dir.to_path_buf();
    }

    let path = PathBuf::from(configured);
    let resolved = if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    };

    if resolved.is_dir() {
        resolved.canonicalize().unwrap_or(resolved)
    } else {
        base_dir.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_engine_extracts_runnable_script() {
        // Имитируем установленный бинарник без скриптов рядом: распаковка вшитой копии.
        //
        // Каталог ОБЯЗАН быть уникальным на процесс. Раньше имя было фиксированным, и любые
        // два параллельных прогона набора (а `cargo mutants -j 4` запускает ровно их) делили
        // один каталог в общем /tmp: один сносил его через remove_dir_all, пока другой
        // распаковывался. Тест падал, и падал ВРАЗБРОС. Цена такого падения не «шум в логе»:
        // покрасневший набор cargo mutants засчитывает как «мутант пойман» — и гейт начинает
        // врать, будто код покрыт, ровно там, где он не покрыт ничем.
        let dir = env::temp_dir().join(format!("clave-engine-embed-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        let path = extract_engine_to(&dir).expect("движок распаковывается");
        assert!(
            path.ends_with(ENGINE_NAME),
            "вернули путь к движку: {path:?}"
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            EMBEDDED_SPEC_CLAVE,
            "содержимое spec-clave совпадает с вшитым"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert!(mode & 0o111 != 0, "spec-clave исполняемый: {mode:o}");
        }

        // Идемпотентность: повторная распаковка не падает и даёт тот же путь.
        assert_eq!(extract_engine_to(&dir).expect("повторно"), path);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolves_dot_to_launch_directory() {
        let base = env::current_dir().expect("test cwd exists");
        assert_eq!(resolve_work_dir(".", &base), base);
    }

    #[test]
    fn resolves_relative_directory_from_launch_directory() {
        let base = env::current_dir().expect("test cwd exists");
        let expected = base.join("src").canonicalize().expect("src dir exists");
        assert_eq!(resolve_work_dir("src", &base), expected);
    }
}
