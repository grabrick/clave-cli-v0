use crate::prelude::*;
use crate::*;

pub(crate) fn main_entry() -> AnyResult<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();

    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_usage();
        return Ok(());
    }

    // Идентификация бинаря (clave-dev снимает её как known-good: первая строка `--version`).
    if args.iter().any(|arg| arg == "-V" || arg == "--version") {
        println!("{APP_COMMAND} v{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Неинтерактивный запуск агента (для внешнего супервайзера clave-dev).
    if args.first().map(String::as_str) == Some("--run") {
        return crate::headless::run_headless(&args[1..]);
    }

    // Неизвестный флаг (например, удалённый `--serve`) не должен молча уходить в движок
    // как «задача» — это запустило бы платный цикл планирования. Задачи натуральным
    // языком с дефиса не начинаются, поэтому показываем справку.
    if let Some(first) = args.first() {
        if first.starts_with('-') {
            eprintln!("clave: unknown option '{first}'\n");
            print_usage();
            return Ok(());
        }
    }

    if !args.is_empty() {
        return run_engine_direct(args);
    }

    run_tui()
}

pub(crate) fn print_usage() {
    println!(
        "{APP_COMMAND}\n\nUsage:\n  {APP_COMMAND}                 Open TUI\n  {APP_COMMAND} <task...>       Run task directly through {ENGINE_NAME}\n  {APP_COMMAND} --help          Show help\n"
    );
}

pub(crate) fn run_engine_direct(args: Vec<String>) -> AnyResult<()> {
    let engine = engine_path().ok_or("spec-clave engine not found")?;
    let work_dir = launch_work_dir();
    let status = Command::new(&engine)
        .current_dir(work_dir)
        .args(args)
        .status()?;
    std::process::exit(status.code().unwrap_or(1));
}

pub(crate) fn run_tui() -> AnyResult<()> {
    force_color_output(true);
    install_panic_hook();
    let _guard = TerminalGuard::new()?;
    let mut app = App::new();
    // Welcome — при пустом старте ИЛИ когда в восстановленном чате нет реального
    // диалога (ни одной реплики «◆ …», напр. после /clear осталась лишь системная
    // строка): новое окно должно встречать приветствием, а не огрызком прошлого чата.
    let has_conversation = app
        .transcript
        .iter()
        .any(|line| line.trim_start().starts_with('◆'));
    if !has_conversation {
        // В файл не пишется (не через push_system) — живёт только в живом блоке.
        app.transcript = welcome_lines(&app);
    }
    let mut renderer = LiveRenderer::new();
    run_app(&mut app, &mut renderer)
}

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
    let _ = execute!(
        io::stdout(),
        DisableBracketedPaste,
        LeaveAlternateScreen,
        DisableMouseCapture,
        crossterm::cursor::Show
    );
}

/// Глобальный panic-hook: при панике ГЛАВНОГО (UI) потока сначала возвращает терминал в
/// норму, затем печатает бэктрейс — иначе он выводится в raw-режиме «лесенкой», а курсор
/// остаётся скрытым. Паники рабочих потоков терминал не трогают и не печатаются: их ловит
/// `spawn_worker` и превращает в WorkerEvent::Failed (иначе бэктрейс испортил бы живой
/// TUI, а лоадер завис бы навсегда).
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if thread::current().name() == Some("main") {
            restore_terminal();
            previous(info);
        }
    }));
}

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
    let shown = path.display().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() => shown
            .strip_prefix(&home)
            .map(|rest| format!("~{rest}"))
            .unwrap_or(shown),
        _ => shown,
    }
}

/// Частота опроса событий: быстрее во время анимаций (плавность), реже в простое (экономия CPU).
pub(crate) fn poll_timeout(animating: bool) -> Duration {
    if animating {
        Duration::from_millis(16)
    } else {
        Duration::from_millis(100)
    }
}

/// Разрыв между итерациями цикла, после которого считаем, что ПК просыпался.
/// Цикл в простое крутится ~раз в 100 мс, активные долгие задачи живут в потоках,
/// так что гэп в несколько секунд бывает только при сне/заморозке процесса.
pub(crate) fn resumed_after_gap(gap: Duration) -> bool {
    gap > Duration::from_secs(3)
}

pub(crate) fn run_app(app: &mut App, renderer: &mut LiveRenderer) -> AnyResult<()> {
    // Часы по «настенному» времени: цикл крутится ~10 раз/сек, поэтому большой
    // разрыв между итерациями = ПК уходил в сон. После пробуждения терминал
    // мог перерисоваться/сдвинуть содержимое, а кэш позиций живого блока устарел
    // → форсим полную перерисовку, иначе блок (футер) дублируется. Работает на
    // любом терминале, не завися от того, прислал ли он Resize/Focus.
    let mut last_tick = std::time::SystemTime::now();
    loop {
        let now = std::time::SystemTime::now();
        if resumed_after_gap(now.duration_since(last_tick).unwrap_or_default()) {
            app.pending_full_redraw = true;
        }
        last_tick = now;

        app.drain_worker_events();
        app.advance_reveal();
        app.expire_footer_notice();
        app.refresh_command_palette_state();
        app.refresh_footer_right_state();
        app.refresh_git_ref();

        let (width, full_h) = crossterm::terminal::size().unwrap_or((80, 24));
        renderer.render(app, width, full_h)?;

        if app.should_quit {
            renderer.clear_for_exit(app)?;
            return Ok(());
        }

        if app.onboarding.is_some() || app.overlay.is_modal() {
            run_modal(app)?;
            renderer.invalidate(); // экран мог измениться — перерисуем блок целиком
            continue;
        }

        if event::poll(poll_timeout(app.is_animating()))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => handle_key(app, key),
                // Вставка целиком (с переносами) идёт в инпут, а не дробится на отправки.
                Event::Paste(text) => {
                    app.finish_reveal_now();
                    app.paste_into_input(&text);
                }
                // Ресайз (в т.ч. после сна ПК / смены монитора): терминал перелил
                // содержимое, кэш позиций живого блока устарел → перерисовать с нуля.
                Event::Resize(_, _) => app.pending_full_redraw = true,
                _ => {}
            }
        }

        if let Some(command) = app.pending_external.take() {
            renderer.leave_below()?; // увести вывод команды под живой блок
            run_external_inline(app, command)?;
            renderer.invalidate();
        }
    }
}

/// Полноэкранная модалка (effort/settings/chats/onboarding) во временном alt-screen
/// со своим Fullscreen-терминалом; живой блок основного экрана сохраняется alt-screen'ом.
fn run_modal(app: &mut App) -> AnyResult<()> {
    execute!(io::stdout(), EnterAlternateScreen)?;
    let result = (|| -> AnyResult<()> {
        let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        while app.onboarding.is_some() || app.overlay.is_modal() {
            app.drain_worker_events();
            terminal.draw(|frame| draw_modal(frame, app))?;
            if app.should_quit {
                break;
            }
            if event::poll(poll_timeout(app.is_animating()))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        handle_key(app, key);
                    }
                }
            }
            if let Some(command) = app.pending_external.take() {
                run_external_inline(app, command)?;
            }
        }
        Ok(())
    })();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
    result
}

fn run_external_inline(app: &mut App, command: ExternalCommand) -> AnyResult<()> {
    let label = app
        .lang
        .choose(command.label_ru, command.label_en)
        .to_string();
    match run_external_command(&command) {
        Ok(code) => {
            let mode = app.mode;
            let lang = app.lang;
            if let Some(onboarding) = app.onboarding.as_mut() {
                onboarding.refresh_auth();
                let ready = auth_requirements_ready(mode, onboarding);
                onboarding.message = if ready {
                    onboarding.step = OnboardingStep::Settings;
                    lang.choose(
                        "Авторизация готова. Проверь стартовые настройки и нажми Enter.",
                        "Authentication is ready. Review startup settings and press Enter.",
                    )
                    .to_string()
                } else if code == 0 {
                    lang.choose(
                        "Логин завершился. Статус обновлен, но нужные аккаунты еще не все готовы.",
                        "Login finished. Status updated, but not every required account is ready yet.",
                    ).to_string()
                } else {
                    lang.choose(
                        "Команда логина завершилась с ошибкой. Проверь текст выше и повтори.",
                        "Login command failed. Check the text above and try again.",
                    )
                    .to_string()
                };
            }
            app.push_system(format!("{label}: exit {code}"));
        }
        Err(err) => app.push_system(format!("{label}: {err}")),
    }
    Ok(())
}

pub(crate) fn handle_key(app: &mut App, key: KeyEvent) {
    // Любое нажатие во время «печати» ответа — мгновенно дорисовать его.
    let was_revealing = app.reveal.is_some();
    app.finish_reveal_now();
    // Если эта клавиша только что до-печатала прозу и открыла селектор — не даём ей
    // же дёрнуть его (иначе Enter мог бы случайно подтвердить первый вариант).
    if was_revealing && app.ask_active() {
        return;
    }

    if app.onboarding.is_some() {
        handle_onboarding_key(app, key);
        return;
    }

    // Активный inline-селектор перехватывает ввод (навигация/выбор/свой ответ).
    if app.ask_active() {
        handle_ask_key(app, key);
        return;
    }

    match app.overlay {
        Overlay::None => handle_input_key(app, key),
        Overlay::Effort => handle_effort_key(app, key),
        Overlay::Settings => handle_settings_key(app, key),
        Overlay::Chats => handle_chats_key(app, key),
        Overlay::Shortcuts => handle_shortcuts_key(app, key),
        Overlay::Search => handle_search_key(app, key),
    }
}

pub(crate) fn handle_input_key(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    // Гейт плана: Enter/Esc имеют особую семантику; остальное — обычный ввод
    // (набор замечания для доработки). Ctrl/Alt-комбинации не перехватываем.
    if app.plan_gate_active() && !ctrl && !alt {
        match key.code {
            KeyCode::Enter => {
                app.submit_plan_gate();
                return;
            }
            KeyCode::Esc => {
                app.cancel_plan();
                return;
            }
            KeyCode::BackTab => return, // режим не меняем, пока открыт гейт
            _ => {}
        }
    }

    if ctrl {
        match key.code {
            KeyCode::Char('c') => app.handle_ctrl_c(),
            KeyCode::Char('j') => app.insert_newline(),
            KeyCode::Char('m') => app.submit_input(),
            KeyCode::Char('a') => app.move_line_start(),
            KeyCode::Char('e') => app.move_line_end(),
            KeyCode::Char('b') => app.move_left(),
            KeyCode::Char('f') => app.move_right(),
            KeyCode::Char('p') => app.history_prev(),
            KeyCode::Char('n') => app.history_next(),
            KeyCode::Char('u') => app.kill_before_cursor(),
            KeyCode::Char('k') => app.kill_after_cursor(),
            KeyCode::Char('w') => app.delete_word_back(),
            KeyCode::Char('d') => app.delete(),
            KeyCode::Char('r') => app.open_search(),
            KeyCode::Left => app.move_word_left(),
            KeyCode::Right => app.move_word_right(),
            KeyCode::Backspace => app.delete_word_back(),
            KeyCode::Delete => app.delete_word_forward(),
            KeyCode::Home => app.cursor = 0,
            KeyCode::End => app.cursor = app.input.len(),
            _ => {}
        }
        return;
    }

    if alt {
        match key.code {
            // Alt/Option+Enter — перенос строки (надёжно различается во всех терминалах).
            KeyCode::Enter => app.insert_newline(),
            KeyCode::Left | KeyCode::Char('b') => app.move_word_left(),
            KeyCode::Right | KeyCode::Char('f') => app.move_word_right(),
            KeyCode::Backspace => app.delete_word_back(),
            KeyCode::Delete | KeyCode::Char('d') => app.delete_word_forward(),
            _ => {}
        }
        return;
    }

    match key.code {
        // Shift+Enter — перенос строки (где терминал сообщает модификатор); обычный
        // Enter отправляет. Ещё варианты переноса: Alt/Option+Enter и Ctrl+J.
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => app.insert_newline(),
        KeyCode::Enter => app.submit_input(),
        KeyCode::Tab => app.complete_command(),
        KeyCode::BackTab => app.chat_mode = app.chat_mode.next(),
        KeyCode::Backspace => app.backspace(),
        KeyCode::Delete => app.delete(),
        KeyCode::Left => app.move_left(),
        KeyCode::Right => app.move_right(),
        // Стрелки умные: в многострочном вводе двигают курсор по строкам, на краю —
        // история (с сохранением черновика). Скролл ленты — нативный (колесо/скролл).
        KeyCode::Up => app.input_up(),
        KeyCode::Down => app.input_down(),
        KeyCode::Home => app.move_line_start(),
        KeyCode::End => app.move_line_end(),
        KeyCode::Esc => {
            app.input.clear();
            app.cursor = 0;
            app.history_index = None;
            app.selected_suggestion = 0;
        }
        KeyCode::Char('?') if app.input.is_empty() => app.overlay = Overlay::Shortcuts,
        KeyCode::Char(ch) if !ch.is_control() => app.insert_char(ch),
        _ => {}
    }
}

pub(crate) fn handle_shortcuts_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        app.handle_ctrl_c();
        return;
    }
    app.overlay = Overlay::None;
}

/// Ввод при открытом inline-селекторе: навигация, отметки (multi), подтверждение,
/// «Свой вариант»/Esc → свободный ответ.
pub(crate) fn handle_ask_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        app.handle_ctrl_c();
        return;
    }
    match key.code {
        KeyCode::Up => app.ask_move(-1),
        KeyCode::Down => app.ask_move(1),
        // Переключение между вопросами (визард на несколько вопросов).
        KeyCode::Tab | KeyCode::Right => app.ask_next(),
        KeyCode::BackTab | KeyCode::Left => app.ask_prev(),
        KeyCode::Enter => app.ask_submit(),
        KeyCode::Esc => app.ask_cancel(),
        KeyCode::Backspace => app.ask_custom_backspace(),
        KeyCode::Char(ch) => {
            if app.ask_on_custom_row() {
                app.ask_custom_push(ch); // печать в поле «своего ответа»
            } else if ch == ' ' {
                app.ask_toggle(); // Space на варианте — отметить (multi)
            }
        }
        _ => {}
    }
}

pub(crate) fn handle_onboarding_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        app.handle_ctrl_c();
        return;
    }

    let Some(step) = app.onboarding.as_ref().map(|onboarding| onboarding.step) else {
        return;
    };

    match step {
        OnboardingStep::Provider => handle_onboarding_provider_key(app, key),
        OnboardingStep::Auth => handle_onboarding_auth_key(app, key),
        OnboardingStep::Settings => handle_onboarding_settings_key(app, key),
    }
}

pub(crate) fn handle_onboarding_provider_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => {
            let index = {
                let onboarding = app.onboarding.as_mut().expect("onboarding exists");
                onboarding.provider_index = onboarding.provider_index.saturating_sub(1);
                onboarding.provider_index
            };
            app.set_mode(provider_mode(index));
        }
        KeyCode::Down => {
            let index = {
                let onboarding = app.onboarding.as_mut().expect("onboarding exists");
                onboarding.provider_index =
                    (onboarding.provider_index + 1).min(provider_count() - 1);
                onboarding.provider_index
            };
            app.set_mode(provider_mode(index));
        }
        KeyCode::Enter => {
            let provider_index = app
                .onboarding
                .as_ref()
                .map(|onboarding| onboarding.provider_index);
            if let Some(provider_index) = provider_index {
                app.set_mode(provider_mode(provider_index));
            }
            if let Some(onboarding) = app.onboarding.as_mut() {
                onboarding.step = OnboardingStep::Auth;
                onboarding.message = app
                    .lang
                    .choose(
                        "Проверь авторизацию CLI. Можно запустить логин прямо отсюда.",
                        "Check CLI authentication. You can run login from here.",
                    )
                    .to_string();
            }
        }
        _ => {}
    }
}

pub(crate) fn handle_onboarding_auth_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('c') | KeyCode::Char('C') => {
            app.pending_external = Some(ExternalCommand {
                program: "codex",
                args: &["login"],
                label_ru: "Codex login",
                label_en: "Codex login",
            });
        }
        KeyCode::Char('l') | KeyCode::Char('L') => {
            app.pending_external = Some(ExternalCommand {
                program: "claude",
                args: &["auth", "login"],
                label_ru: "Claude auth login",
                label_en: "Claude auth login",
            });
        }
        KeyCode::Enter => {
            if let Some(onboarding) = app.onboarding.as_mut() {
                onboarding.step = OnboardingStep::Settings;
                onboarding.message = app
                    .lang
                    .choose(
                        "Выставь стартовые настройки. Enter сохранит конфиг.",
                        "Choose startup defaults. Enter saves the config.",
                    )
                    .to_string();
            }
        }
        KeyCode::Backspace | KeyCode::Esc => {
            if let Some(onboarding) = app.onboarding.as_mut() {
                onboarding.step = OnboardingStep::Provider;
            }
        }
        _ => {}
    }
}

pub(crate) fn handle_onboarding_settings_key(app: &mut App, key: KeyEvent) {
    let setting_index = app
        .onboarding
        .as_ref()
        .map(|onboarding| onboarding.setting_index)
        .unwrap_or(0);

    match key.code {
        KeyCode::Up => {
            if let Some(onboarding) = app.onboarding.as_mut() {
                onboarding.setting_index = onboarding.setting_index.saturating_sub(1);
            }
        }
        KeyCode::Down => {
            if let Some(onboarding) = app.onboarding.as_mut() {
                onboarding.setting_index = (onboarding.setting_index + 1).min(2);
            }
        }
        KeyCode::Left => adjust_onboarding_setting(app, setting_index, -1),
        KeyCode::Right => adjust_onboarding_setting(app, setting_index, 1),
        KeyCode::Char('l') | KeyCode::Char('L') => {
            app.lang = if app.lang == Language::Ru {
                Language::En
            } else {
                Language::Ru
            };
        }
        KeyCode::Enter => {
            app.onboarding = None;
            app.status = app.lang.choose("готов", "ready").to_string();
            app.save_current_config(true);
        }
        KeyCode::Backspace | KeyCode::Esc => {
            if let Some(onboarding) = app.onboarding.as_mut() {
                onboarding.step = OnboardingStep::Auth;
            }
        }
        _ => {}
    }
}

pub(crate) fn adjust_onboarding_setting(app: &mut App, setting_index: usize, direction: isize) {
    match setting_index {
        0 => {
            if direction < 0 {
                app.rounds = app.rounds.saturating_sub(1).max(1);
            } else {
                app.rounds = (app.rounds + 1).min(9);
            }
        }
        1 => {
            app.adjust_startup_effort(direction);
        }
        2 => {
            app.lang = if app.lang == Language::Ru {
                Language::En
            } else {
                Language::Ru
            };
        }
        _ => {}
    }
}

pub(crate) fn handle_effort_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.effort_focus = app.effort_focus.saturating_sub(1),
        KeyCode::Down => {
            app.effort_focus = (app.effort_focus + 1).min(app.effort_picker_rows() - 1);
        }
        KeyCode::Left => app.adjust_effort_focus(-1),
        KeyCode::Right => app.adjust_effort_focus(1),
        KeyCode::Enter => {
            app.overlay = Overlay::None;
            app.effort_original = None;
            app.status = app.lang.choose("готов", "ready").to_string();
            app.save_current_config(true);
            app.push_command_result(format!("Set to {}", app.effort_summary()));
        }
        KeyCode::Esc => {
            if let Some(snapshot) = app.effort_original.take() {
                app.restore_effort_snapshot(snapshot);
            }
            app.overlay = Overlay::None;
            app.status = app.lang.choose("готов", "ready").to_string();
            app.push_command_result("Cancelled");
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.handle_ctrl_c();
        }
        _ => {}
    }
}

pub(crate) fn handle_search_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if matches!(key.code, KeyCode::Char('c')) {
            app.handle_ctrl_c();
        }
        return;
    }
    match key.code {
        KeyCode::Esc => app.close_search(),
        KeyCode::Enter | KeyCode::Down => app.search_step(1),
        KeyCode::Up => app.search_step(-1),
        KeyCode::Backspace => app.search_backspace(),
        KeyCode::Char(ch) if !ch.is_control() => app.search_input(ch),
        _ => {}
    }
}

pub(crate) fn handle_chats_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.chats_index = app.chats_index.saturating_sub(1),
        KeyCode::Down => {
            let last = app.chats_picker.len().saturating_sub(1);
            app.chats_index = (app.chats_index + 1).min(last);
        }
        KeyCode::Enter => {
            let selected = app
                .chats_picker
                .get(app.chats_index)
                .map(|chat| chat.id.clone());
            app.overlay = Overlay::None;
            if let Some(id) = selected {
                app.resume_chat(&id);
            }
        }
        KeyCode::Esc => app.overlay = Overlay::None,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.handle_ctrl_c();
        }
        _ => {}
    }
}

pub(crate) fn handle_settings_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.adjust_settings_focus(-1),
        KeyCode::Down => app.adjust_settings_focus(1),
        KeyCode::Left => app.adjust_settings_value(-1),
        KeyCode::Right => app.adjust_settings_value(1),
        KeyCode::Enter => {
            app.overlay = Overlay::None;
            app.settings_original = None;
            app.status = app.lang.choose("готов", "ready").to_string();
            app.save_current_config(true);
            app.push_command_result(format!("Saved {}", app.settings_summary()));
        }
        KeyCode::Esc => {
            if let Some(snapshot) = app.settings_original.take() {
                app.restore_settings_snapshot(snapshot);
            }
            app.overlay = Overlay::None;
            app.status = app.lang.choose("готов", "ready").to_string();
            app.push_command_result("Cancelled");
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.handle_ctrl_c();
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_timeout_is_shorter_during_animation() {
        assert!(poll_timeout(true) < poll_timeout(false));
        assert_eq!(poll_timeout(true), Duration::from_millis(16));
        assert_eq!(poll_timeout(false), Duration::from_millis(100));
    }

    #[test]
    fn gap_detects_sleep_but_not_normal_idle() {
        // Обычные итерации цикла (≤100 мс) и небольшая возня — не сон.
        assert!(!resumed_after_gap(Duration::from_millis(100)));
        assert!(!resumed_after_gap(Duration::from_millis(900)));
        // Многосекундный разрыв = ПК спал → полная перерисовка.
        assert!(resumed_after_gap(Duration::from_secs(5)));
        assert!(resumed_after_gap(Duration::from_secs(3600)));
    }

    #[test]
    fn restored_chat_reaches_transcript_for_the_live_region() {
        // Реальный файл чата на диске -> restore_or_create_chat -> transcript,
        // откуда живой регион (flush_overflow/draw_viewport) его и показывает.
        let dir = env::temp_dir().join(format!("clave-startup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");

        let id = "chat-startup-7";
        let path = chat_path_for_id(&dir, id);
        save_chat_transcript(
            &path,
            id,
            &[
                "◆ MARKER_OLD_CHAT".to_string(),
                "⏺ STALE_ANSWER".to_string(),
            ],
        )
        .expect("save chat");

        let (rid, _, transcript) = restore_or_create_chat(&dir, Some(id), Language::Ru);
        assert_eq!(rid, id);
        assert_eq!(
            transcript,
            vec![
                "◆ MARKER_OLD_CHAT".to_string(),
                "⏺ STALE_ANSWER".to_string()
            ],
            "восстановленный чат обязан попасть в transcript"
        );

        // Пустой старт → transcript пуст (run_tui подставит welcome_lines).
        let (_, _, fresh) = restore_or_create_chat(&dir, None, Language::Ru);
        assert!(fresh.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }
}
