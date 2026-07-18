use super::*;

/// RAII: гарантированно снимает raw mode и сбрасывает терминал (alt-screen, mouse —
/// на случай, если modal их включал) при любом выходе или панике (инвариант 6).
pub(crate) struct TerminalGuard;

impl TerminalGuard {
    pub(crate) fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        // Bracketed paste: терминал оборачивает вставку в маркеры и crossterm отдаёт
        // её одним Event::Paste — иначе переносы строк в тексте приходят как Enter и
        // дробят вставку на несколько отправок.
        let _ = execute!(io::stdout(), EnableBracketedPaste);
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

/// Возвращает терминал в нормальное состояние: снимает raw mode, выключает bracketed
/// paste / alt-screen / mouse capture (на случай, если их включала модалка) и — важно —
/// СНОВА показывает курсор. Рендер прячет курсор через Hide; без явного Show он остался
/// бы невидимым после аварийного выхода или паники.
fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = restore_screen(&mut io::stdout());
}

/// Escape-последовательности возврата экрана в норму. Шов ради тестов: пишем в ЛЮБОЙ приёмник,
/// а не только в настоящий stdout. Иначе проверить, что мы вообще что-то шлём — и, главное, что
/// среди этого есть Show курсора, — можно было бы только глазами на живом терминале. То есть
/// никак: мутационный прогон показал, что вся эта функция заменяется пустышкой, и ни один тест
/// не замечает. А цена такой поломки — сломанный терминал у пользователя после каждой аварии.
fn restore_screen(out: &mut impl Write) -> io::Result<()> {
    execute!(
        out,
        DisableBracketedPaste,
        LeaveAlternateScreen,
        DisableMouseCapture,
        crossterm::cursor::Show
    )
}

/// Кто имеет право трогать терминал при панике. Только ГЛАВНЫЙ (UI) поток: рабочий поток пишет
/// в живой TUI, и его бэктрейс изуродовал бы экран, а лоадер завис бы навсегда.
fn panic_touches_terminal(thread_name: Option<&str>) -> bool {
    thread_name == Some("main")
}

/// Глобальный panic-hook: при панике ГЛАВНОГО (UI) потока сначала возвращает терминал в
/// норму, затем печатает бэктрейс — иначе он выводится в raw-режиме «лесенкой», а курсор
/// остаётся скрытым. Паники рабочих потоков терминал не трогают и не печатаются: их ловит
/// `spawn_worker` и превращает в WorkerEvent::Failed (иначе бэктрейс испортил бы живой
/// TUI, а лоадер завис бы навсегда).
pub(crate) fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if panic_touches_terminal(thread::current().name()) {
            restore_terminal();
            previous(info);
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Байты, которые обязаны уйти в терминал. Замерены на живом crossterm, а не выдуманы.
    const SHOW_CURSOR: &str = "\u{1b}[?25h";
    const LEAVE_ALT_SCREEN: &str = "\u{1b}[?1049l";
    const DISABLE_PASTE: &str = "\u{1b}[?2004l";
    const DISABLE_MOUSE: &str = "\u{1b}[?1000l";

    const TERM_CASE: &str = "CLAVE_TEST_TERMINAL_CASE";
    const TERM_SELF: &str =
        "runtime::terminal::tests::the_terminal_is_really_restored_on_exit_and_on_panic";

    #[test]
    fn restoring_the_screen_shows_the_cursor_and_leaves_the_alt_screen() {
        let mut out = Vec::new();
        restore_screen(&mut out).expect("восстановление пишет в приёмник");
        let seq = String::from_utf8_lossy(&out);

        assert!(
            seq.contains(SHOW_CURSOR),
            "курсор ОБЯЗАН вернуться: рендер прятал его через Hide, и без Show пользователь \
             остался бы с невидимым курсором. Ушло: {seq:?}"
        );
        assert!(
            seq.contains(LEAVE_ALT_SCREEN),
            "не вышли из alt-screen: {seq:?}"
        );
        assert!(
            seq.contains(DISABLE_PASTE),
            "bracketed paste не выключен: {seq:?}"
        );
        assert!(
            seq.contains(DISABLE_MOUSE),
            "захват мыши не выключен: {seq:?}"
        );
    }

    #[test]
    fn only_a_panic_of_the_main_thread_touches_the_terminal() {
        // Рабочий поток пишет в ЖИВОЙ TUI: его бэктрейс изуродовал бы экран, а лоадер завис бы
        // навсегда. Поэтому терминал трогает только паника главного (UI) потока.
        assert!(panic_touches_terminal(Some("main")));
        assert!(!panic_touches_terminal(Some("clave-worker")));
        assert!(!panic_touches_terminal(None));
    }

    /// Прогнать один случай в ДОЧЕРНЕМ процессе и вернуть весь его вывод.
    ///
    /// Иначе никак: `restore_terminal` пишет в НАСТОЯЩИЙ `io::stdout()`, мимо перехвата
    /// тестового харнесса. Проверить, что вызов вообще состоялся, можно только со стороны —
    /// поймав вывод отдельного процесса.
    ///
    /// `--test-threads=1` обязателен: только так libtest гоняет тест на потоке `main`, а
    /// panic-hook именно по имени потока и решает, трогать ли терминал.
    fn run_terminal_case(case: &str) -> String {
        let exe = std::env::current_exe().expect("путь к тестовому бинарю");
        let out = std::process::Command::new(exe)
            .args([TERM_SELF, "--exact", "--nocapture", "--test-threads=1"])
            .env(TERM_CASE, case)
            .output()
            .expect("дочерний тест не запустился");
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr)
    }

    #[test]
    fn the_terminal_is_really_restored_on_exit_and_on_panic() {
        match std::env::var(TERM_CASE).ok().as_deref() {
            // Ребёнок: зовём восстановление по-настоящему. Родитель поймает его stdout.
            Some("exit") => restore_terminal(),
            // Ребёнок: печатаем справку. `print_usage` — тонкая обёртка над println!, и её
            // целиком заменяли пустышкой: `clave --help` мог печатать пустоту, а CI бы это
            // пропустил. Проверить факт печати можно только со стороны.
            Some("usage") => print_usage(),
            // Ребёнок: роняем RAII-стража. Он обязан вернуть терминал при ОБЫЧНОМ выходе — не
            // только при панике. Его `drop` тоже заменялся пустышкой бесследно.
            //
            // Создаём напрямую, минуя `new()`: тот включает raw-режим, а у дочернего процесса
            // stdout — не терминал, и `new()` вернул бы ошибку. Нам нужен именно `drop`.
            Some("guard") => {
                let guard = TerminalGuard;
                drop(guard);
            }
            // Ребёнок: ставим хук и роняем поток, НАЗВАННЫЙ `main`. Восстановление обязано
            // случиться САМО — иначе после любой паники clave оставляет пользователю сломанный
            // терминал.
            //
            // Почему не паниковать прямо тут: libtest гоняет тело теста в отдельном потоке,
            // названном ПО ИМЕНИ ТЕСТА, — и даже `--test-threads=1` этого не меняет (замерено:
            // хук отработал как «не главный поток» и не сделал ничего). А хук решает именно по
            // имени потока. Поэтому воспроизводим ровно то условие, которое он стережёт.
            Some("panic") => {
                install_panic_hook();
                let dying = std::thread::Builder::new()
                    .name("main".to_string())
                    .spawn(|| panic!("нарочная паника: терминал обязан вернуться сам"))
                    .expect("поток запущен");
                assert!(dying.join().is_err(), "поток обязан был упасть");
            }
            _ => {
                let on_exit = run_terminal_case("exit");
                assert!(
                    on_exit.contains("1 passed"),
                    "дочерний тест обязан РЕАЛЬНО прогнаться; коду возврата тут верить нельзя — \
                     с опечаткой в фильтре ребёнок гоняет ноль тестов и выходит нулём:\n{on_exit}"
                );
                assert!(
                    on_exit.contains(SHOW_CURSOR),
                    "restore_terminal не написал в терминал НИЧЕГО — то есть его можно заменить \
                     пустышкой, и после аварийного выхода курсор останется невидимым:\n{on_exit}"
                );

                let on_panic = run_terminal_case("panic");
                assert!(
                    on_panic.contains("1 passed"),
                    "дочерний случай «panic» обязан РЕАЛЬНО прогнаться:\n{on_panic}"
                );
                assert!(
                    on_panic.contains("нарочная паника"),
                    "паника обязана быть напечатана: хук цепляет предыдущий обработчик, и без \
                     этого бэктрейс терялся бы молча:\n{on_panic}"
                );
                assert!(
                    on_panic.contains(SHOW_CURSOR),
                    "после паники главного потока терминал НЕ восстановлен — значит panic-hook \
                     не ставится или не зовёт восстановление, и пользователь остаётся со \
                     сломанным терминалом:\n{on_panic}"
                );

                let on_usage = run_terminal_case("usage");
                assert!(
                    on_usage.contains("1 passed"),
                    "дочерний случай «usage» обязан РЕАЛЬНО прогнаться:\n{on_usage}"
                );
                assert!(
                    on_usage.contains("Open TUI") && on_usage.contains("--help"),
                    "print_usage не напечатал НИЧЕГО — то есть `clave --help` может молча \
                     показывать пустоту:\n{on_usage}"
                );

                let on_guard = run_terminal_case("guard");
                assert!(
                    on_guard.contains("1 passed"),
                    "дочерний случай «guard» обязан РЕАЛЬНО прогнаться:\n{on_guard}"
                );
                assert!(
                    on_guard.contains(SHOW_CURSOR),
                    "RAII-страж не вернул терминал при выходе из области видимости — значит его \
                     Drop можно заменить пустышкой, и терминал останется сломанным даже при \
                     ОБЫЧНОМ завершении, без всякой паники:\n{on_guard}"
                );
            }
        }
    }
}
