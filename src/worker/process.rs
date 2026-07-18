use crate::prelude::*;
use crate::*;
use std::sync::atomic::{AtomicU64, Ordering};

/// Настраивает дочерний процесс лидером НОВОЙ группы процессов, чтобы его собственные
/// под-процессы (Bash-инструменты агента) попадали в ту же группу. Тогда при отмене или
/// таймауте всё дерево убивается разом (см. `kill_process_tree`), а не только сам CLI.
/// На не-unix — no-op (там дерево завершает `child.kill()`).
#[cfg(unix)]
pub(crate) fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
pub(crate) fn configure_process_group(_command: &mut Command) {}

/// Убивает дочерний процесс ВМЕСТЕ со всей его группой (внуками) и пожинает зомби.
/// На unix шлём SIGKILL всей группе через отрицательный pid: процесс спавнился лидером
/// группы (`configure_process_group`), значит pgid == его pid, и цель сигнала — `-pid`.
/// Это закрывает stdout/stderr-пайпы, которые мог держать под-процесс агента, — иначе
/// тред-ридер завис бы на `read` навсегда.
pub(crate) fn kill_process_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        // SAFETY: kill(2) с отрицательным pid адресует группу процессов; аргументы —
        // простые скаляры, предусловий на память нет.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
    let _ = child.wait();
}

/// Спавнит рабочий поток, гарантируя терминальное событие даже при панике его тела.
/// Без этого паника воркера оставила бы `running=true` и вечный лоадер (главный поток
/// продолжает жить). Панику ловим (её вывод подавлен в panic-hook) и шлём Failed.
pub(crate) fn spawn_worker<F>(fail_tx: Sender<WorkerEvent>, body: F)
where
    F: FnOnce() + Send + 'static,
{
    thread::spawn(move || {
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)).is_err() {
            let _ = fail_tx.send(WorkerEvent::Failed(
                "внутренняя ошибка Clave (паника рабочего потока)".to_string(),
            ));
        }
    });
}

/// Приватный подкаталог для временных файлов (0700 на unix): вывод codex кладём туда,
/// чтобы на многопользовательской машине его нельзя было подменить симлинком до записи —
/// каталог принадлежит нам, посторонний в него не запишет. Если создать не удалось,
/// падаем на общий temp_dir (лучше работать, чем упасть).
pub(crate) fn private_temp_dir() -> PathBuf {
    let dir = env::temp_dir().join(format!("clave-{}", std::process::id()));
    if fs::create_dir_all(&dir).is_err() {
        return env::temp_dir();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    }
    dir
}

/// Временный файл для `-o` codex: удаляется при выходе из области видимости по ЛЮБОЙ
/// ветке (успех, отмена, таймаут, ошибка спавна). Раньше `remove_file` был рассыпан по
/// веткам — забыть его в новой ветке означало утечку файла в /tmp на каждый прогон.
pub(crate) struct TempOut {
    path: PathBuf,
}

impl TempOut {
    /// Имя уникально в пределах процесса за счёт счётчика (часы могут не дать
    /// уникальности двум соседним вызовам — два шага тандема писали бы в один файл),
    /// а между процессами — за счёт pid в имени каталога (`private_temp_dir`).
    pub(crate) fn new(prefix: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        Self {
            path: private_temp_dir().join(format!("{prefix}-{seq}.txt")),
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Содержимое файла (пусто, если провайдер его не создавал — например claude).
    pub(crate) fn read(&self) -> String {
        fs::read_to_string(&self.path).unwrap_or_default()
    }
}

impl Drop for TempOut {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Простой дольше лимита — провайдер считается зависшим (граница включительная).
pub(crate) fn idle_expired(elapsed: Duration, timeout: Duration) -> bool {
    elapsed >= timeout
}

/// Сколько простаивает провайдер (отравленный mutex → 0: живой процесс не убиваем зря).
pub(crate) fn idle_elapsed(last_activity: &Arc<Mutex<Instant>>) -> Duration {
    last_activity
        .lock()
        .map(|t| t.elapsed())
        .unwrap_or_default()
}

pub(crate) fn idle_timeout_message(lang: Language) -> String {
    idle_timeout_message_for(lang, idle_timeout())
}

/// Текст «провайдер молчал N секунд» в отрыве от окружения (тест прибивает секунды).
fn idle_timeout_message_for(lang: Language, timeout: Duration) -> String {
    format!(
        "{} {}{}",
        lang.choose("Провайдер не отвечал", "Provider produced no output for"),
        timeout.as_secs(),
        lang.choose(" c — остановлен по таймауту.", "s — stopped (timeout)."),
    )
}

pub(crate) fn spawn_reader<R>(reader: R, tx: Sender<WorkerEvent>)
where
    R: io::Read + Send + 'static,
{
    thread::spawn(move || {
        let reader = BufReader::new(reader);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    let _ = tx.send(WorkerEvent::Line(line));
                }
                Err(err) => {
                    let _ = tx.send(WorkerEvent::Line(format!("read error: {err}")));
                    break;
                }
            }
        }
    });
}

/// Лимит простоя: нет вывода дольше него → провайдер считается зависшим и
/// убивается (иначе застрявший CLI висит до ручного Ctrl+C). Это «тишина», а не
/// общий таймаут — нормальный агентский прогон постоянно стримит события, поэтому
/// долгие, но активные раны не страдают. Переопределяется `CLAVE_IDLE_TIMEOUT_SECS`.
pub(crate) fn idle_timeout() -> Duration {
    idle_timeout_from(env::var("CLAVE_IDLE_TIMEOUT_SECS").ok().as_deref())
}

/// Разбор лимита простоя из значения переменной. Мусор и ноль → дефолт: нулевой таймаут
/// убивал бы провайдера сразу после спавна, отрицательный/нечисловой — просто опечатка.
fn idle_timeout_from(value: Option<&str>) -> Duration {
    let secs = value
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&s| s > 0)
        .unwrap_or(180);
    Duration::from_secs(secs)
}

/// Отметить «была активность сейчас» (ридеры зовут на каждой строке вывода).
pub(crate) fn touch_activity(last: &Arc<Mutex<Instant>>) {
    if let Ok(mut guard) = last.lock() {
        *guard = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_out_lives_in_private_dir_and_is_removed() {
        let out = TempOut::new("test-out");
        let path = out.path().to_path_buf();
        assert!(
            path.starts_with(env::temp_dir()),
            "временный файл лежит в приватном каталоге: {path:?}"
        );

        fs::write(&path, "ответ codex").expect("файл записывается");
        assert_eq!(out.read(), "ответ codex", "read() отдаёт содержимое файла");

        // Два файла подряд не совпадают: иначе два шага тандема писали бы в один.
        let other = TempOut::new("test-out");
        assert_ne!(other.path(), path.as_path());

        drop(out);
        assert!(!path.exists(), "Drop удаляет файл — иначе утечка в /tmp");
    }

    #[test]
    fn temp_out_read_is_empty_when_provider_wrote_nothing() {
        // claude файл `-o` не создаёт: чтение обязано дать пустоту, а не панику.
        let out = TempOut::new("test-missing");
        assert_eq!(out.read(), "");
    }

    #[test]
    fn idle_expired_boundary_is_inclusive() {
        let limit = Duration::from_secs(180);
        assert!(idle_expired(limit, limit), "ровно лимит — уже зависание");
        assert!(idle_expired(Duration::from_millis(180_001), limit));
        assert!(!idle_expired(Duration::from_millis(179_999), limit));
        assert!(!idle_expired(Duration::ZERO, limit));
    }

    #[test]
    fn activity_resets_the_idle_clock() {
        let stale = Instant::now()
            .checked_sub(Duration::from_secs(600))
            .expect("часы позволяют отмотать назад");
        let last = Arc::new(Mutex::new(stale));
        assert!(
            idle_elapsed(&last) >= Duration::from_secs(600),
            "простой считается от последней активности"
        );

        // Строка вывода = активность: живой процесс не должен попасть под таймаут.
        touch_activity(&last);
        assert!(idle_elapsed(&last) < Duration::from_secs(1));
    }

    #[test]
    fn idle_timeout_falls_back_on_zero_and_garbage() {
        let default = Duration::from_secs(180);
        assert_eq!(idle_timeout_from(None), default);
        assert_eq!(idle_timeout_from(Some("5")), Duration::from_secs(5));
        assert_eq!(idle_timeout_from(Some("1")), Duration::from_secs(1));
        // Ноль убивал бы провайдера сразу после спавна.
        assert_eq!(idle_timeout_from(Some("0")), default);
        assert_eq!(idle_timeout_from(Some("-1")), default);
        assert_eq!(idle_timeout_from(Some("abc")), default);
        // Живой лимит: нулевой таймаут = убийство любого прогона.
        assert!(idle_timeout() > Duration::ZERO);
    }

    #[test]
    fn idle_timeout_message_names_the_limit() {
        let ru = idle_timeout_message_for(Language::Ru, Duration::from_secs(45));
        assert!(ru.contains("45") && ru.contains("таймауту"), "{ru}");
        let en = idle_timeout_message_for(Language::En, Duration::from_secs(45));
        assert!(en.contains("45") && en.contains("timeout"), "{en}");
        // Обёртка берёт лимит из окружения, а не из воздуха.
        assert_eq!(
            idle_timeout_message(Language::Ru),
            idle_timeout_message_for(Language::Ru, idle_timeout())
        );
    }

    #[test]
    fn spawn_reader_forwards_every_line() {
        let (tx, rx) = mpsc::channel();
        spawn_reader(io::Cursor::new("первая\nвторая\n"), tx);
        let lines: Vec<String> = rx
            .iter()
            .map(|event| match event {
                WorkerEvent::Line(line) => line,
                _ => panic!("ожидали Line"),
            })
            .collect();
        assert_eq!(lines, ["первая", "вторая"]);
    }

    #[test]
    fn spawn_worker_runs_body_and_reports_panic() {
        let (tx, rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        spawn_worker(tx.clone(), move || {
            let _ = done_tx.send(());
        });
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("тело воркера выполнено");

        // Паника воркера обязана дать терминальное событие — иначе вечный лоадер.
        spawn_worker(tx, || panic!("бум"));
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(WorkerEvent::Failed(msg)) => assert!(msg.contains("паника"), "{msg}"),
            other => panic!("ожидали Failed после паники: {}", other.is_ok()),
        }
    }

    // Критично: при отмене/таймауте мы убиваем ВСЮ группу процессов, а не только сам
    // CLI. Модель «процесс → под-процесс в той же группе»: внук должен умереть вместе с
    // прямым потомком — иначе он держал бы stdout-пайп и тред-ридер завис бы навсегда.
    #[cfg(unix)]
    #[test]
    fn kill_process_tree_reaps_grandchild() {
        let marker = env::temp_dir().join(format!("clave-test-pgrp-{}.pid", std::process::id()));
        let _ = fs::remove_file(&marker);

        // Дочерний sh порождает фоновый sleep (внук) в той же группе и печатает его PID.
        let script = format!("sleep 30 & echo $! > {}; wait", marker.to_string_lossy());
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(script)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        configure_process_group(&mut command);
        let mut child = command.spawn().expect("sh запускается");

        let grandchild = wait_for_pid(&marker);
        assert!(
            process_alive(grandchild),
            "внук должен быть жив до kill_process_tree"
        );

        kill_process_tree(&mut child);

        // После убийства группы внук должен исчезнуть в пределах таймаута.
        let mut dead = false;
        for _ in 0..200 {
            if !process_alive(grandchild) {
                dead = true;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let _ = fs::remove_file(&marker);
        assert!(
            dead,
            "внук ({grandchild}) пережил kill_process_tree — группа не убита"
        );
    }

    #[cfg(unix)]
    fn wait_for_pid(marker: &Path) -> i32 {
        for _ in 0..300 {
            if let Ok(text) = fs::read_to_string(marker) {
                if let Ok(pid) = text.trim().parse::<i32>() {
                    if pid > 0 {
                        return pid;
                    }
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("внук не записал PID за отведённое время");
    }

    #[cfg(unix)]
    fn process_alive(pid: i32) -> bool {
        // kill(pid, 0) сигнал не шлёт — только проверяет существование процесса.
        unsafe { libc::kill(pid, 0) == 0 }
    }
}
