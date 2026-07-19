use crate::prelude::*;
use crate::*;

/// Проверяет логин ОДНОГО провайдера (для воркера, без заморозки UI).
pub(crate) fn provider_authenticated(provider: Provider) -> bool {
    match provider {
        Provider::Claude => claude_auth_probe().authenticated,
        Provider::Codex => codex_auth_probe().authenticated,
    }
}

/// Лёгкая проверка наличия бинарника провайдера в PATH — без запуска процесса,
/// поэтому безопасно звать из UI-потока (в отличие от `*_auth_probe`).
pub(crate) fn provider_binary_present(provider: &str) -> bool {
    let name = match provider {
        "claude" => claude_binary(),
        "codex" => codex_binary(),
        _ => return false,
    };
    if name.contains('/') {
        return std::path::Path::new(&name).is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(&name).is_file()))
}

pub(crate) fn codex_auth_probe() -> AuthProbe {
    auth_probe(&codex_binary(), &["login", "status"], auth_timeout())
}

pub(crate) fn claude_auth_probe() -> AuthProbe {
    auth_probe(
        &claude_binary(),
        &["auth", "status", "--text"],
        auth_timeout(),
    )
}

/// Потолок ожидания пробы логина. Пробу зовут СИНХРОННО: из `App::new()` — ещё до того,
/// как нарисован хоть один кадр, — и из обработчика клавиш, когда терминал уже в raw-режиме.
/// Раньше тут стоял `Command::output()`, который ждёт выхода процесса и EOF на его потоках,
/// то есть буквально вечно: подвисший на сети `claude`/`codex` морозил Clave насмерть — без
/// экрана, без вывода, без объяснения. Переопределяется `CLAVE_AUTH_TIMEOUT_SECS`.
fn auth_timeout() -> Duration {
    auth_timeout_from(env::var("CLAVE_AUTH_TIMEOUT_SECS").ok().as_deref())
}

/// Разбор потолка из значения переменной. Мусор и ноль → дефолт: нулевой потолок убивал бы
/// пробу сразу после спавна, нечисловой — просто опечатка. Десяти секунд хватает даже
/// провайдеру, который лезет за токеном в сеть.
fn auth_timeout_from(value: Option<&str>) -> Duration {
    let secs = value
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&s| s > 0)
        .unwrap_or(10);
    Duration::from_secs(secs)
}

/// Статус немого провайдера. Обязан назвать причину: иначе в онбординге он выглядит как
/// «не залогинен», и пользователь идёт чинить логин, с которым всё в порядке.
fn auth_timeout_status(timeout: Duration) -> String {
    format!("no response in {}s", timeout.as_secs())
}

/// Проба логина с потолком по времени. Бинарь и потолок — ПАРАМЕТРЫ, а не окружение: иначе
/// «зависший CLI нас не морозит» проверялось бы только настоящим зависшим CLI, то есть никак.
fn auth_probe(binary: &str, args: &[&str], timeout: Duration) -> AuthProbe {
    let mut command = Command::new(binary);
    command
        .args(args)
        // Как делал `output()`: проба не смеет воровать ввод у TUI.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Лидерство в группе тут не украшение: `kill_process_tree` шлёт сигнал ГРУППЕ (`-pid`),
    // и без него сигнал не дошёл бы ни до кого, а `wait` внутри повис бы навсегда — починка
    // обернулась бы новым зависанием.
    configure_process_group(&mut command);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            return AuthProbe {
                installed: false,
                authenticated: false,
                status: err.to_string(),
            }
        }
    };

    // Потоки читаем в тредах: молчаливый CLI не забьёт пайп, а болтливый не упрётся в его
    // буфер (`output()` делал ровно это, только без потолка).
    let stdout = child.stdout.take().map(spawn_capture_reader);
    let stderr = child.stderr.take().map(spawn_capture_reader);
    let deadline = Instant::now() + timeout;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // CLI вышел — его собственный вывод уже в пайпе и читается мгновенно.
                // Но если внук унаследовал stdout/stderr и держит их открытыми,
                // read_to_string не дождётся EOF; безусловный join висел бы здесь до
                // выхода внука (исторический вечный фриз старта — пробу зовут из
                // App::new ещё до первого кадра). Поэтому собираем с коротким потолком
                // и, не дождавшись, отпускаем (детач) ридеры: child уже реапнут, и
                // kill по его возможно-переиспользованному PID был бы опасен.
                let out = join_with_ceiling(stdout);
                let err = join_with_ceiling(stderr);
                let text = command_output_text(out.as_bytes(), err.as_bytes());
                return AuthProbe {
                    installed: true,
                    authenticated: auth_output_looks_ready(status.success(), &text),
                    status: first_nonempty_line(&text)
                        .unwrap_or_else(|| "status unavailable".to_string()),
                };
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    // Убиваем группу целиком: внук, унаследовавший stdout, держал бы пайп
                    // открытым — тогда ридеры не дождались бы EOF, и join завис бы.
                    // Поэтому их не join-им, а роняем: их результат нам уже не нужен.
                    kill_process_tree(&mut child);
                    drop(stdout);
                    drop(stderr);
                    return AuthProbe {
                        installed: true,
                        authenticated: false,
                        status: auth_timeout_status(timeout),
                    };
                }
                thread::sleep(Duration::from_millis(20));
            }
            Err(err) => {
                kill_process_tree(&mut child);
                return AuthProbe {
                    installed: true,
                    authenticated: false,
                    status: err.to_string(),
                };
            }
        }
    }
}

/// Забирает вывод ридера с коротким потолком. Вызывается, когда CLI УЖЕ вышел, — его
/// вывод обычно приходит мгновенно. Но `handle.join()` напрямую висел бы вечно, если
/// внук унаследовал пайп и держит его открытым (нет EOF). Джойн уводим в отдельный
/// тред, а результат ждём через канал с `recv_timeout`: не дождавшись — возвращаем
/// пусто, а зависший джойн-тред отпускаем (он завершится, когда внук закроет пайп).
fn join_with_ceiling(handle: Option<thread::JoinHandle<String>>) -> String {
    let Some(handle) = handle else {
        return String::new();
    };
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(handle.join().unwrap_or_default());
    });
    rx.recv_timeout(Duration::from_millis(200))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Фальшивый провайдер БЕЗ временного файла: скрипт уходит в `/bin/sh -c`.
    ///
    /// Первая версия писала свежий `.sh` и запускала его — и это стоило по 200–400 мс НА КАЖДЫЙ
    /// вызов: macOS проверяет (подпись/Gatekeeper) каждый впервые исполняемый файл, и кэш тут не
    /// помогает, потому что файл каждый раз новый. Три таких теста растянули весь набор с
    /// полутора до пяти с половиной секунд — а под `cargo mutants` набор гоняется на КАЖДОГО из
    /// 2400 мутантов, и полный замер ядра раздулся с 22 минут до шести часов.
    ///
    /// `/bin/sh` система уже проверила и закэшировала: спавн стоит 3 мс. Заодно исчезает мусор в
    /// /tmp, на который я же и жаловался.
    #[cfg(unix)]
    fn fake_provider(script: &'static str) -> (&'static str, [&'static str; 2]) {
        ("/bin/sh", ["-c", script])
    }

    /// Пробу гоняем в отдельном потоке и ждём с запасом. Смысл в том, что при откате
    /// починки тест ОБЯЗАН УПАСТЬ, а не повиснуть: набор, висящий вечно, ничего не
    /// сообщает — он просто вешает CI.
    #[cfg(unix)]
    fn probe_in_thread(script: &'static str, timeout: Duration) -> Receiver<AuthProbe> {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let (binary, args) = fake_provider(script);
            let _ = tx.send(auth_probe(binary, &args, timeout));
        });
        rx
    }

    // Провайдер замолчал — И оставил под собой внука, как делает настоящий CLI, поднимая свои
    // под-процессы. Один тест ловит сразу две вещи:
    //
    //   1. потолок срабатывает. Раньше `.output()` ждал бы вечно, а пробу зовут из `App::new()`
    //      ещё до первого кадра — подвисший CLI морозил всю Clave на старте: пустой экран и
    //      никаких объяснений.
    //
    //   2. убивается ВСЯ группа. Без `configure_process_group` процесс не лидер группы, сигнал
    //      `kill(-pid)` не дошёл бы ни до кого, и `wait()` внутри `kill_process_tree` завис бы
    //      на внуке (sh ждёт свой `sleep 30`). Проба вернулась бы через 30 с, а не через одну, —
    //      и `recv_timeout(15 с)` этот случай ловит.
    //
    // Гонки тут нет намеренно: тест ничего не выясняет про внука до срабатывания потолка.
    // Первая версия пыталась — читала pid внука из файла-маркера, пока проба ждёт, — и
    // разваливалась под нагрузкой (`sh` не успевал стартовать за отведённые секунды). Хуже
    // того: падая на ровном месте, она красила набор и заставляла cargo mutants записывать
    // мутантов в «пойманные», которых никто не ловил. Флейк не просто шумит — он ВРЁТ гейту,
    // и врёт в сторону «всё покрыто».
    #[cfg(unix)]
    #[test]
    fn a_mute_provider_with_a_grandchild_does_not_freeze_the_probe() {
        let rx = probe_in_thread("sleep 30 & wait", Duration::from_secs(1));

        let probe = rx.recv_timeout(Duration::from_secs(15)).expect(
            "проба обязана вернуться по потолку — а вернуться она может, только если убита \
             ВСЯ группа: иначе wait() внутри kill_process_tree ждал бы внука полминуты",
        );

        assert!(probe.installed, "бинарь запустился — значит, установлен");
        assert!(
            !probe.authenticated,
            "немой провайдер не может считаться залогиненным"
        );
        assert_eq!(
            probe.status, "no response in 1s",
            "статус обязан назвать причину: иначе это выглядит как «не залогинен», \
             и пользователь пойдёт чинить логин, с которым всё в порядке"
        );
    }

    // Внук унаследовал stdout, а сам CLI ВЫШЕЛ мгновенно (exit 0). try_wait даёт
    // Ok(Some(0)) сразу, но read_to_string ридера не видит EOF — внук держит пайп.
    // Раньше безусловный join() в ветке выхода висел бы до самого выхода внука (для
    // настоящего фонового демона — фактически вечно), морозя старт Clave. Потолок
    // join_with_ceiling обязан вернуть пробу быстро.
    #[cfg(unix)]
    #[test]
    fn a_fast_exit_leaving_a_grandchild_on_the_pipe_does_not_freeze_the_probe() {
        // `sleep 30 &` — внук с унаследованным stdout; `exit 0` — sh выходит сразу.
        let rx = probe_in_thread("sleep 30 & exit 0", Duration::from_secs(1));
        let probe = rx.recv_timeout(Duration::from_secs(5)).expect(
            "проба обязана вернуться, не дожидаясь EOF от внука: безусловный join висел \
             бы до самого выхода внука",
        );
        assert!(probe.installed, "процесс запустился — бинарь установлен");
    }

    #[cfg(unix)]
    #[test]
    fn probe_takes_the_answer_of_a_live_provider() {
        let (binary, args) = fake_provider("echo 'Logged in as user@example.com'");
        let probe = auth_probe(binary, &args, Duration::from_secs(10));

        assert!(probe.installed);
        assert!(
            probe.authenticated,
            "явный маркер логина обязан приниматься"
        );
        assert_eq!(probe.status, "Logged in as user@example.com");
    }

    // stderr и ненулевой код: провайдер ругается не в stdout, и потолок тут ни при чём —
    // ответ обязан дойти целиком.
    #[cfg(unix)]
    #[test]
    fn probe_takes_stderr_and_the_exit_code() {
        let (binary, args) = fake_provider("echo 'You are not logged in.' >&2; exit 1");
        let probe = auth_probe(binary, &args, Duration::from_secs(10));

        assert!(probe.installed, "процесс запустился — бинарь на месте");
        assert!(!probe.authenticated);
        assert_eq!(probe.status, "You are not logged in.");
    }

    #[test]
    fn probe_reports_a_missing_binary() {
        let probe = auth_probe(
            "/nonexistent/clave-no-such-provider",
            &[],
            Duration::from_secs(1),
        );
        assert!(!probe.installed, "несуществующий бинарь — не установлен");
        assert!(!probe.authenticated);
        assert!(!probe.status.is_empty(), "причина обязана быть названа");
    }

    // Потолок, схлопнувшийся в ноль, — это не «строгая проверка», а поломка: пробу убивало бы
    // сразу после спавна, и КАЖДЫЙ провайдер выглядел бы немым. Ловушка не гипотетическая:
    // `Duration::default()` и есть ноль, и мутационный гейт показал, что без этого теста
    // такую подмену не замечает никто.
    #[test]
    fn auth_timeout_is_never_zero() {
        assert!(
            auth_timeout() > Duration::ZERO,
            "нулевой потолок объявил бы немым любого провайдера, даже здорового"
        );
    }

    #[test]
    fn auth_timeout_falls_back_on_zero_and_garbage() {
        assert_eq!(auth_timeout_from(Some("30")), Duration::from_secs(30));
        // Ноль убивал бы пробу сразу после спавна — провайдер не успел бы и слова сказать.
        assert_eq!(auth_timeout_from(Some("0")), Duration::from_secs(10));
        assert_eq!(auth_timeout_from(Some("-5")), Duration::from_secs(10));
        assert_eq!(auth_timeout_from(Some("вечность")), Duration::from_secs(10));
        assert_eq!(auth_timeout_from(None), Duration::from_secs(10));
    }

    #[test]
    fn auth_timeout_status_names_the_limit() {
        assert_eq!(
            auth_timeout_status(Duration::from_secs(7)),
            "no response in 7s"
        );
    }
}
