use crate::prelude::*;
use crate::*;

mod input;
mod terminal;
mod welcome;
pub(crate) use input::*;
pub(crate) use terminal::*;
pub(crate) use welcome::*;

/// Куда уходит запуск по аргументам командной строки.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Launch {
    Usage,
    Version,
    /// Неизвестный флаг — с его именем, чтобы назвать пользователю.
    UnknownFlag(String),
    /// Задача натуральным языком → встроенный движок.
    Engine,
    Tui,
    /// `--resume`/`-r`: TUI сразу с открытым списком сохранённых чатов.
    ResumeTui,
}

/// Разбор аргументов в РЕШЕНИЕ — отдельно от его исполнения.
///
/// Шов ради тестов: `main_entry` дальше запускает TUI или движок, и проверить разбор
/// на живом процессе значило бы на каждый флаг поднимать настоящий clave. Мутационный прогон
/// показал, что этого не делает никто: восемь подмен в условиях (`==` → `!=`, `||` → `&&`,
/// удалённый `!`) проходили бесследно. Цена — `--help` перестаёт работать, а задача уходит не
/// туда: неизвестный флаг молча уехал бы в движок «задачей» и запустил ПЛАТНЫЙ цикл планирования.
pub(crate) fn launch_for(args: &[String]) -> Launch {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        return Launch::Usage;
    }
    if args.iter().any(|arg| arg == "-V" || arg == "--version") {
        return Launch::Version;
    }
    // `--resume`/`-r`: открыть TUI сразу на списке сохранённых чатов (иначе флаг ушёл бы в
    // UnknownFlag ниже — resume существовал только как команда `/resume` внутри TUI).
    if args.iter().any(|arg| arg == "-r" || arg == "--resume") {
        return Launch::ResumeTui;
    }
    // Неизвестный флаг (например, удалённый `--serve`) не должен молча уходить в движок как
    // «задача»: задачи натуральным языком с дефиса не начинаются.
    if let Some(first) = args.first() {
        if first.starts_with('-') {
            return Launch::UnknownFlag(first.clone());
        }
    }
    if !args.is_empty() {
        return Launch::Engine;
    }
    Launch::Tui
}

pub(crate) fn main_entry() -> AnyResult<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();

    match launch_for(&args) {
        Launch::Usage => {
            print_usage();
            Ok(())
        }
        Launch::Version => {
            println!("{APP_COMMAND} v{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Launch::UnknownFlag(flag) => {
            eprintln!("clave: unknown option '{flag}'\n");
            print_usage();
            Ok(())
        }
        Launch::Engine => run_engine_direct(args),
        Launch::Tui => run_tui(false),
        Launch::ResumeTui => run_tui(true),
    }
}

pub(crate) fn print_usage() {
    println!("{}", usage_text());
}

/// Нужно ли встретить пользователя приветствием: в ленте нет ни одной реплики («◆ …»).
///
/// Пустой старт ИЛИ восстановленный чат без диалога (после `/clear` там остаётся только
/// системная строка) — новое окно обязано показать приветствие, а не огрызок прошлого чата.
///
/// Отрицание тут несущее, а не косметическое: без него приветствие ЗАТЁРЛО БЫ живой диалог при
/// восстановлении чата, а пустой старт остался бы с голым экраном. Мутационный прогон показал,
/// что удаление `!` не замечает никто.
fn needs_welcome(transcript: &[String]) -> bool {
    !transcript
        .iter()
        .any(|line| line.trim_start().starts_with('◆'))
}

/// Текст справки. Отдельно от печати: `print_usage` целиком заменялась пустышкой, и никто не
/// замечал — то есть `clave --help` мог начать печатать пустоту, а CI бы это пропустил.
pub(crate) fn usage_text() -> String {
    format!(
        "{APP_COMMAND}\n\nUsage:\n  {APP_COMMAND}                 Open TUI\n  {APP_COMMAND} --resume        Open TUI on the saved chats list\n  {APP_COMMAND} <task...>       Run task directly through {ENGINE_NAME}\n  {APP_COMMAND} --help          Show help\n"
    )
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

pub(crate) fn run_tui(open_resume: bool) -> AnyResult<()> {
    force_color_output(true);
    install_panic_hook();
    let _guard = TerminalGuard::new()?;
    let mut app = App::new();
    app.refresh_git_ref();
    if needs_welcome(&app.transcript) {
        // В файл не пишется (не через push_system) — живёт только в живом блоке.
        app.transcript = welcome_lines(&app);
    }
    // `clave --resume`: сразу открыть список сохранённых чатов (пусто — покажет подсказку).
    if open_resume {
        app.open_chats_picker();
    }
    let mut renderer = LiveRenderer::new();
    run_app(&mut app, &mut renderer)
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

        let (width, full_h) = crossterm::terminal::size().unwrap_or((80, 24));
        renderer.render(app, width, full_h)?;

        if app.should_quit {
            renderer.clear_for_exit(app)?;
            return Ok(());
        }

        if wants_modal(app) {
            run_modal(app)?;
            renderer.invalidate(); // экран мог измениться — перерисуем блок целиком
            continue;
        }

        if event::poll(poll_timeout(app.is_animating()))? {
            apply_event(app, event::read()?);
        }

        if let Some(command) = app.pending_external.take() {
            renderer.leave_below()?; // увести вывод команды под живой блок
            run_external_inline(app, command)?;
            renderer.invalidate();
        }
    }
}

/// Экран рисуется полноэкранной модалкой, а не живым блоком. Условие ИЛИ, а не И: онбординг и
/// оверлей — независимые причины уйти в alt-screen, и любой из них хватает. С «И» онбординг без
/// оверлея (то есть первый запуск!) рисовался бы живым блоком поверх ленты.
fn wants_modal(app: &App) -> bool {
    app.onboarding.is_some() || app.overlay.is_modal()
}

/// Нажатие клавиши из события: `None` — это не клавиатура ИЛИ это ОТПУСКАНИЕ.
///
/// Фильтр по `kind` обязателен: Windows шлёт и Press, и Release, и без него каждая клавиша
/// сработала бы ДВАЖДЫ — напечатал «а», получил «аа».
fn key_press(event: Event) -> Option<KeyEvent> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => Some(key),
        _ => None,
    }
}

/// Что одно событие терминала делает с состоянием ОСНОВНОГО экрана. Вынесено из петли отдельно от
/// `event::read()`: иначе проверить, что вставка попадает в инпут ЦЕЛИКОМ (а не дробится на
/// отправки по переносам) и что отпускание клавиши не печатает второй раз, можно было бы только
/// настоящим терминалом. Мутационный прогон показал цену: обе ветки — Paste и Resize — удалялись
/// бесследно.
///
/// В МОДАЛКЕ этого делать нельзя, и потому она зовёт `key_press` напрямую: там нет инпута, и
/// вставка молча накапливалась бы в буфере главного экрана, чтобы вывалиться при закрытии.
fn apply_event(app: &mut App, event: Event) {
    match event {
        Event::Paste(text) => {
            app.finish_reveal_now();
            // Вставка идёт в тот буфер, что СЕЙЧАС владеет вводом, а не всегда в главный
            // композер: иначе поверх открытого поиска/inline-селектора текст молча уходил
            // не туда и всплывал в композере при закрытии оверлея. Управляющие символы
            // (переносы) в однострочные поля не тащим — как и обычный ввод клавишами.
            if app.ask_active() {
                if app.ask_on_custom_row() {
                    for ch in text.chars().filter(|c| !c.is_control()) {
                        app.ask_custom_push(ch);
                    }
                }
            } else {
                match app.overlay {
                    // Главный композер: вставка целиком (с переносами), не дробится на отправки.
                    Overlay::None => app.paste_into_input(&text),
                    Overlay::Search => {
                        for ch in text.chars().filter(|c| !c.is_control()) {
                            app.search_input(ch);
                        }
                    }
                    // Оверлеи без текстового ввода (подсказки и пр.) вставку игнорируют.
                    _ => {}
                }
            }
        }
        // Ресайз (в т.ч. после сна ПК / смены монитора): терминал перелил содержимое,
        // кэш позиций живого блока устарел → перерисовать с нуля.
        Event::Resize(_, _) => app.pending_full_redraw = true,
        other => {
            if let Some(key) = key_press(other) {
                handle_key(app, key);
            }
        }
    }
}

/// Полноэкранная модалка (effort/settings/chats/onboarding) во временном alt-screen
/// со своим Fullscreen-терминалом; живой блок основного экрана сохраняется alt-screen'ом.
fn run_modal(app: &mut App) -> AnyResult<()> {
    execute!(io::stdout(), EnterAlternateScreen)?;
    let result = (|| -> AnyResult<()> {
        let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        while wants_modal(app) {
            app.drain_worker_events();
            terminal.draw(|frame| draw_modal(frame, app))?;
            if app.should_quit {
                break;
            }
            if event::poll(poll_timeout(app.is_animating()))? {
                // ТОЛЬКО клавиши. Не `apply_event`: в модалке нет инпута, и вставка молча
                // накапливалась бы в буфере главного экрана, чтобы вывалиться при закрытии.
                // Фильтр Press/Release общий с основной петлёй — чтобы они не разъехались.
                if let Some(key) = key_press(event::read()?) {
                    handle_key(app, key);
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

/// Что сказать человеку после внешней команды логина. `code` — её код возврата.
///
/// Исходов ТРИ, и путать их нельзя: всё готово / логин прошёл, но нужных аккаунтов ещё не все /
/// сама команда упала. Подмена `code == 0` на `!=` меняет два последних местами — человек пошёл
/// бы чинить упавшую команду, которая на самом деле отработала, и наоборот. Мутационный прогон
/// показал, что эту подмену не замечал никто.
fn login_message(ready: bool, code: i32, lang: Language) -> &'static str {
    if ready {
        return lang.choose(
            "Авторизация готова. Проверь стартовые настройки и нажми Enter.",
            "Authentication is ready. Review startup settings and press Enter.",
        );
    }
    if code == 0 {
        return lang.choose(
            "Логин завершился. Статус обновлен, но нужные аккаунты еще не все готовы.",
            "Login finished. Status updated, but not every required account is ready yet.",
        );
    }
    lang.choose(
        "Команда логина завершилась с ошибкой. Проверь текст выше и повтори.",
        "Login command failed. Check the text above and try again.",
    )
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
                if ready {
                    onboarding.step = OnboardingStep::Settings;
                }
                onboarding.message = login_message(ready, code, lang).to_string();
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
    // же дёрнуть его (иначе Enter мог бы случайно подтвердить первый вариант). Но
    // Ctrl+C пропускаем: пользователь обязан мочь прервать/выйти этой же клавишей,
    // иначе нажатие, открывшее селектор, глотало бы и попытку остановиться.
    if was_revealing && app.ask_active() {
        let is_ctrl_c =
            key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c'));
        if !is_ctrl_c {
            return;
        }
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
        Overlay::Plugins => handle_plugins_key(app, key),
        Overlay::Shortcuts => handle_shortcuts_key(app, key),
        Overlay::Search => handle_search_key(app, key),
    }
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

/// Клавиши панели плагинов: навигация, поиск (прямой ввод), действия и подтверждение.
pub(crate) fn handle_plugins_key(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    // Ctrl+C — выход из приложения из любого таба/суб-состояния.
    if ctrl && matches!(key.code, KeyCode::Char('c')) {
        app.handle_ctrl_c();
        return;
    }

    // Суб-состояния таба «Источники» перехватывают ввод целиком.
    if app.marketplace_input.is_some() {
        match key.code {
            KeyCode::Enter => app.marketplace_submit_add(),
            KeyCode::Esc => app.marketplace_cancel_input(),
            KeyCode::Tab => app.marketplace_toggle_add_provider(),
            KeyCode::Backspace => app.marketplace_input_backspace(),
            KeyCode::Char(c) if !ctrl => app.marketplace_input_push(c),
            _ => {}
        }
        return;
    }
    if app.marketplace_confirm.is_some() {
        match key.code {
            KeyCode::Enter => app.confirm_marketplace_remove(),
            KeyCode::Esc => app.cancel_marketplace_remove(),
            _ => {}
        }
        return;
    }
    // Подтверждение действия над плагином (табы «Установленные»/«Каталог»).
    if app.plugins_confirm.is_some() {
        match key.code {
            KeyCode::Enter => app.confirm_plugin_action(),
            KeyCode::Esc => app.cancel_plugin_action(),
            _ => {}
        }
        return;
    }

    // Переключение табов — общее. В Каталоге цифры уходят в поиск, поэтому там прыжок только Tab.
    match key.code {
        KeyCode::Tab => return app.plugins_tab_next(),
        KeyCode::BackTab => return app.plugins_tab_prev(),
        _ => {}
    }

    match app.plugins_tab {
        PluginsTab::Overview => handle_overview_key(app, key),
        PluginsTab::Installed | PluginsTab::Catalog => handle_plugin_list_key(app, key, ctrl),
        PluginsTab::Sources => handle_sources_key(app, key),
    }
}

/// Таб «Обзор»: `↑↓` по строкам сводки, `Enter`/цифра — прыжок в таб, `Esc` — закрыть.
fn handle_overview_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.overview_index = app.overview_index.saturating_sub(1),
        KeyCode::Down => {
            let last = OVERVIEW_ROWS.len().saturating_sub(1);
            app.overview_index = (app.overview_index + 1).min(last);
        }
        KeyCode::Enter => app.overview_enter(),
        KeyCode::Char(c) => {
            if let Some(tab) = tab_for_digit(c) {
                app.set_plugins_tab(tab);
            }
        }
        KeyCode::Esc => app.overlay = Overlay::None,
        _ => {}
    }
}

/// Табы «Установленные»/«Каталог»: список плагинов. Действия на Ctrl-клавишах; в Каталоге буквы
/// уходят в поиск, в Установленных (поиска нет) цифры листают табы.
fn handle_plugin_list_key(app: &mut App, key: KeyEvent, ctrl: bool) {
    match key.code {
        KeyCode::Up => app.plugins_index = app.plugins_index.saturating_sub(1),
        KeyCode::Down => {
            let last = app.filtered_plugins().len().saturating_sub(1);
            app.plugins_index = (app.plugins_index + 1).min(last);
        }
        // ←/→ — переключить провайдера (Claude ⇄ Codex); иначе codex тонет под каталогом claude.
        KeyCode::Left | KeyCode::Right => app.toggle_plugins_provider(),
        KeyCode::Enter => app.plugin_enter(),
        KeyCode::Char('e') if ctrl => app.plugin_toggle(),
        KeyCode::Char('u') if ctrl => app.plugin_update(),
        KeyCode::Char(c) => {
            if app.plugins_tab == PluginsTab::Catalog {
                app.plugins_query.push(c);
                app.plugins_index = 0;
            } else if let Some(tab) = tab_for_digit(c) {
                app.set_plugins_tab(tab);
            }
        }
        KeyCode::Backspace if app.plugins_tab == PluginsTab::Catalog => {
            app.plugins_query.pop();
            app.plugins_index = 0;
        }
        KeyCode::Esc => {
            // В Каталоге Esc сначала снимает поиск, затем закрывает панель.
            if app.plugins_tab == PluginsTab::Catalog && !app.plugins_query.is_empty() {
                app.plugins_query.clear();
                app.plugins_index = 0;
            } else {
                app.overlay = Overlay::None;
            }
        }
        _ => {}
    }
}

/// Таб «Источники»: `a` — добавить, `Enter` — удалить (через подтверждение), цифра — прыжок в таб.
fn handle_sources_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.marketplaces_index = app.marketplaces_index.saturating_sub(1),
        KeyCode::Down => {
            let last = app.marketplaces.len().saturating_sub(1);
            app.marketplaces_index = (app.marketplaces_index + 1).min(last);
        }
        KeyCode::Char('a') => app.marketplace_start_add(),
        KeyCode::Enter => app.marketplace_enter_remove(),
        KeyCode::Char(c) => {
            if let Some(tab) = tab_for_digit(c) {
                app.set_plugins_tab(tab);
            }
        }
        KeyCode::Esc => app.overlay = Overlay::None,
        _ => {}
    }
}

/// Цифра `1`–`4` → таб по позиции в баре (быстрый прыжок вне Каталога).
fn tab_for_digit(c: char) -> Option<PluginsTab> {
    let index = c.to_digit(10)?.checked_sub(1)? as usize;
    PluginsTab::ALL.get(index).copied()
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
pub(crate) mod keytest {
    use super::*;

    /// App для тестов клавиатуры. Собираем через `App::from_config` на своих временных путях
    /// и с `onboarding_done = true`: `App::new()` читал бы пользовательский конфиг и при
    /// невыполненном онбординге поднимал `Onboarding::new` — то есть настоящие auth-probe
    /// процессы codex/claude. Дальше фиксируем поля, которые читают обработчики клавиш.
    pub(crate) fn app_for_keys() -> App {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = env::temp_dir().join(format!(
            "clave-keys-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::create_dir_all(&dir);

        let config = AppConfig {
            onboarding_done: true,
            ..AppConfig::default()
        };
        let mut app = App::from_config(
            config,
            dir.join("config.json"),
            dir.join("history"),
            dir.clone(),
        );
        app.chat_path = dir.join("chat.md");
        app.lang = Language::Ru;
        app.onboarding = None;
        app.overlay = Overlay::None;
        app.chat_mode = ChatMode::Discussion;
        app.input.clear();
        app.cursor = 0;
        app.transcript.clear();
        app.history.clear();
        app.history_index = None;
        app.history_draft = None;
        app.selected_suggestion = 0;
        app.pending_plan = None;
        app.plan_flow = PlanFlow::None;
        app.pending_messages.clear();
        app.running = false;
        app.should_quit = false;
        app.last_ctrl_c_at = None;
        app
    }

    /// App с активным гейтом плана (`pending_plan` + `!running`).
    pub(crate) fn app_with_plan_gate() -> App {
        let mut app = app_for_keys();
        app.pending_plan = Some(PendingPlan {
            task: "задача".to_string(),
            plan: "шаг 1".to_string(),
        });
        app
    }

    /// App с активным гейтом тандема (`tandem_gate` + `running`) и приёмником, чтобы
    /// проверить, какое решение ушло заблокированному воркеру.
    pub(crate) fn app_with_tandem_gate() -> (App, std::sync::mpsc::Receiver<TandemGate>) {
        let mut app = app_for_keys();
        let (tx, rx) = std::sync::mpsc::channel();
        app.running = true;
        app.tandem_gate = true;
        app.tandem_gate_tx = Some(tx);
        (app, rx)
    }

    #[test]
    fn tandem_gate_enter_approves_execution() {
        let (mut app, rx) = app_with_tandem_gate();
        handle_input_key(&mut app, key(KeyCode::Enter));
        assert_eq!(
            rx.try_recv().ok(),
            Some(TandemGate::Execute),
            "Enter на гейте → исполнить последнюю версию"
        );
        assert!(!app.tandem_gate, "гейт закрыт после решения");
    }

    #[test]
    fn tandem_gate_esc_aborts_without_writing() {
        let (mut app, rx) = app_with_tandem_gate();
        handle_input_key(&mut app, key(KeyCode::Esc));
        assert_eq!(
            rx.try_recv().ok(),
            Some(TandemGate::Abort),
            "Esc на гейте → отмена без записи"
        );
        assert!(!app.tandem_gate, "гейт закрыт после решения");
    }

    #[test]
    fn tandem_gate_ignores_ctrl_combinations() {
        // Ctrl+Enter НЕ одобряет исполнение: комбинации редактора/прерывания сюда не
        // относятся, иначе случайный Ctrl+Enter молча запускал бы запись.
        let (mut app, rx) = app_with_tandem_gate();
        handle_input_key(&mut app, ctrl(KeyCode::Enter));
        assert!(rx.try_recv().is_err(), "Ctrl+Enter решение не шлёт");
        assert!(app.tandem_gate, "Ctrl+Enter гейт не закрывает");
    }

    /// App с активным ВВОД-гейтом тандема + приёмники ответа и отмены.
    pub(crate) fn app_with_tandem_input_gate() -> (
        App,
        std::sync::mpsc::Receiver<String>,
        std::sync::mpsc::Receiver<()>,
    ) {
        let mut app = app_for_keys();
        let (in_tx, in_rx) = std::sync::mpsc::channel();
        let (cancel_tx, cancel_rx) = std::sync::mpsc::channel();
        app.running = true;
        app.tandem_input_gate = true;
        app.tandem_input_tx = Some(in_tx);
        app.cancel_tx = Some(cancel_tx);
        (app, in_rx, cancel_rx)
    }

    #[test]
    fn tandem_input_gate_enter_sends_typed_answer() {
        let (mut app, in_rx, _cancel_rx) = app_with_tandem_input_gate();
        app.input = "почини баг в X".to_string();
        app.cursor = app.input.len();
        handle_input_key(&mut app, key(KeyCode::Enter));
        assert_eq!(
            in_rx.try_recv().ok().as_deref(),
            Some("почини баг в X"),
            "Enter шлёт набранный ответ воркеру"
        );
        assert!(!app.tandem_input_gate, "гейт закрыт после ответа");
        assert!(app.input.is_empty(), "инпут очищен");
    }

    #[test]
    fn tandem_input_gate_empty_answer_is_ignored() {
        // Пустой ответ не отправляем — ждём текст (иначе воркер получил бы пустую строку).
        let (mut app, in_rx, _cancel_rx) = app_with_tandem_input_gate();
        app.input = "   ".to_string();
        handle_input_key(&mut app, key(KeyCode::Enter));
        assert!(in_rx.try_recv().is_err(), "пустой ответ не уходит");
        assert!(app.tandem_input_gate, "гейт остаётся открыт");
    }

    #[test]
    fn tandem_input_gate_esc_cancels_tandem() {
        let (mut app, _in_rx, cancel_rx) = app_with_tandem_input_gate();
        handle_input_key(&mut app, key(KeyCode::Esc));
        assert!(cancel_rx.try_recv().is_ok(), "Esc отменяет тандем");
        assert!(!app.tandem_input_gate, "гейт закрыт");
    }

    #[test]
    fn tandem_input_gate_passes_typing_through() {
        // Обычные символы на ввод-гейте — это НАБОР ответа, а не спецклавиши.
        let (mut app, in_rx, _cancel_rx) = app_with_tandem_input_gate();
        handle_input_key(&mut app, key(KeyCode::Char('a')));
        assert_eq!(app.input, "a", "символ ушёл в набор ответа");
        assert!(in_rx.try_recv().is_err(), "набор ничего не отправляет");
        assert!(app.tandem_input_gate, "гейт открыт, пока печатаешь");
    }

    pub(crate) fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    pub(crate) fn key_with(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    pub(crate) fn ctrl(code: KeyCode) -> KeyEvent {
        key_with(code, KeyModifiers::CONTROL)
    }

    pub(crate) fn alt(code: KeyCode) -> KeyEvent {
        key_with(code, KeyModifiers::ALT)
    }
}

#[cfg(test)]
mod tests {
    use super::keytest::*;
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

        // Старт в ПУСТОМ каталоге → transcript пуст (run_tui подставит welcome_lines). Каталог
        // отдельный: в `dir` уже лежит чат, и per-directory fallback его бы восстановил.
        let empty_dir = env::temp_dir().join(format!("clave-startup-empty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&empty_dir);
        fs::create_dir_all(&empty_dir).expect("temp dir");
        let (_, _, fresh) = restore_or_create_chat(&empty_dir, None, Language::Ru);
        assert!(fresh.is_empty());

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&empty_dir);
    }

    // ───────────────────────── гейт плана ─────────────────────────

    #[test]
    fn plan_gate_esc_cancels_plan() {
        let mut app = app_with_plan_gate();
        handle_input_key(&mut app, key(KeyCode::Esc));
        assert!(app.pending_plan.is_none(), "Esc на гейте отменяет план");
        assert!(
            app.transcript
                .iter()
                .any(|line| line.contains("План отменён")),
            "отмена видна в ленте: {:?}",
            app.transcript
        );
    }

    #[test]
    fn plan_gate_ignores_ctrl_and_alt_combinations() {
        // Гейт перехватывает только чистые клавиши: Ctrl+Esc и Alt+Esc до cancel_plan
        // не доходят, иначе комбинации редактора рушили бы план.
        let mut with_ctrl = app_with_plan_gate();
        handle_input_key(&mut with_ctrl, ctrl(KeyCode::Esc));
        assert!(
            with_ctrl.pending_plan.is_some(),
            "Ctrl+Esc план не отменяет"
        );

        let mut with_alt = app_with_plan_gate();
        handle_input_key(&mut with_alt, alt(KeyCode::Esc));
        assert!(with_alt.pending_plan.is_some(), "Alt+Esc план не отменяет");
    }

    #[test]
    fn plan_gate_backtab_keeps_chat_mode() {
        let mut app = app_with_plan_gate();
        handle_input_key(&mut app, key(KeyCode::BackTab));
        assert_eq!(
            app.chat_mode,
            ChatMode::Discussion,
            "пока открыт гейт, BackTab режим не переключает"
        );
        assert!(app.pending_plan.is_some());
    }

    #[test]
    fn plan_gate_passes_plain_typing_to_editor() {
        // Обычный символ на гейте — это набор замечания, а не отмена плана.
        let mut app = app_with_plan_gate();
        handle_input_key(&mut app, key(KeyCode::Char('к')));
        assert_eq!(app.input, "к");
        assert!(app.pending_plan.is_some());
    }

    #[test]
    fn enter_without_gate_submits_input() {
        // running = true → start_chat кладёт сообщение в очередь и НЕ поднимает провайдер.
        let mut app = app_for_keys();
        app.running = true;
        app.input = "привет".to_string();
        app.cursor = app.input.len();
        handle_input_key(&mut app, key(KeyCode::Enter));
        assert!(app.input.is_empty(), "инпут очищен отправкой");
        assert_eq!(
            app.pending_messages.front().map(String::as_str),
            Some("привет")
        );
    }

    #[test]
    fn a_gap_of_exactly_the_limit_is_not_yet_a_sleep() {
        // Граница СТРОГАЯ: ровно три секунды — это ещё не сон, а четыре уже сон. Подмена `>` на
        // `>=` двигает её на шаг и заставляла бы перерисовывать экран на ровном месте.
        assert!(!resumed_after_gap(Duration::from_secs(3)));
        assert!(resumed_after_gap(Duration::from_millis(3001)));
    }

    #[test]
    fn welcome_greets_by_name_version_and_working_dir() {
        // Приветствие целиком заменялось на пустой список — новое окно встречало бы человека
        // голым экраном, и никто бы не заметил.
        let app = app_for_keys();
        let lines = welcome_lines(&app);

        assert!(!lines.is_empty(), "приветствие не может быть пустым");
        let all = lines.join("\n");
        assert!(
            all.contains("clave"),
            "приветствие обязано назвать себя: {all:?}"
        );
        assert!(
            all.contains(env!("CARGO_PKG_VERSION")),
            "приветствие обязано показать версию: {all:?}"
        );
        assert!(
            all.contains("/help"),
            "приветствие обязано подсказать, где искать команды: {all:?}"
        );
        // Логотип — не украшение: без него строки схлопнутся и вёрстка поедет.
        assert!(all.contains('█'), "логотип на месте: {all:?}");
    }

    // ─────────────────────────── ПРИВЕТСТВИЕ И ЛОГИН ───────────────────────────

    #[test]
    fn welcome_greets_an_empty_start_but_never_overwrites_a_live_chat() {
        // Отрицание тут несущее: без него приветствие ЗАТЁРЛО БЫ восстановленный диалог, а
        // пустой старт остался бы с голым экраном.
        assert!(
            needs_welcome(&[]),
            "пустой старт обязан встречать приветствием"
        );
        assert!(
            needs_welcome(&["система: чат очищен".to_string()]),
            "после /clear остаётся только системная строка — это ещё не диалог"
        );
        assert!(
            !needs_welcome(&["◆ привет".to_string()]),
            "в восстановленном чате есть реплика — приветствие затёрло бы её"
        );
        assert!(
            !needs_welcome(&["  ◆ реплика с отступом".to_string()]),
            "отступ перед репликой ничего не меняет"
        );
    }

    #[test]
    fn login_message_tells_the_three_outcomes_apart() {
        // Исходов три, и путать их нельзя: человек пойдёт чинить не то.
        let ok = login_message(true, 0, Language::Ru);
        assert!(ok.contains("готова"), "всё готово: {ok}");

        // Команда отработала (код 0), но нужных аккаунтов ещё не все.
        let partial = login_message(false, 0, Language::Ru);
        assert!(
            partial.contains("не все готовы"),
            "логин прошёл, но не всё: {partial}"
        );

        // Сама команда упала — это ДРУГОЕ, и текст обязан отличаться.
        let failed = login_message(false, 1, Language::Ru);
        assert!(failed.contains("ошибкой"), "команда упала: {failed}");
        assert_ne!(
            partial, failed,
            "«логин прошёл, но не всё» и «команда упала» — разные беды, и путать их нельзя"
        );

        assert!(login_message(true, 0, Language::En).contains("ready"));
    }

    // ─────────────────────────── СЕЛЕКТОР И ДИСПЕТЧЕР ───────────────────────────
    //
    // Хелперы селектора (`ask_question`, `app_with_ask`, `ask_state`) лежат ниже, рядом с
    // тестами `handle_ask_key`.

    #[test]
    fn an_open_selector_still_receives_keys() {
        // Условие «клавиша до-печатала прозу И открыла селектор → съесть её» держится на И.
        // С ИЛИ оно срабатывало бы от одного лишь открытого селектора — и тот перестал бы
        // отвечать на клавиши ВООБЩЕ: стрелки не двигают выбор, Enter не подтверждает.
        //
        // Идём через `handle_key` (диспетчер), а не через `handle_ask_key` напрямую: проверяется
        // именно МАРШРУТ до селектора.
        let mut app = app_with_ask(vec![ask_question(
            "Что делаем?",
            false,
            &["первый", "второй"],
        )]);
        assert_eq!(
            ask_state(&app).answers[0].cursor,
            0,
            "старт на первом варианте"
        );

        handle_key(&mut app, key(KeyCode::Down));

        assert_ne!(
            ask_state(&app).answers[0].cursor,
            0,
            "открытый селектор обязан отвечать на клавиши — иначе выбрать в нём нельзя ничего"
        );
    }

    // ─────────────────────────── ПЕТЛЯ СОБЫТИЙ ───────────────────────────

    #[test]
    fn onboarding_and_a_modal_overlay_each_call_the_modal_on_their_own() {
        // Условие ИЛИ, а не И, и это не придирка: с «И» онбординг без оверлея — то есть ПЕРВЫЙ
        // ЗАПУСК — рисовался бы живым блоком поверх ленты вместо полноэкранной модалки.
        let mut only_overlay = app_for_keys();
        only_overlay.overlay = Overlay::Effort;
        assert!(
            wants_modal(&only_overlay),
            "оверлей сам по себе требует модалку"
        );

        let mut only_onboarding = app_for_keys();
        only_onboarding.onboarding = Some(Onboarding::new(only_onboarding.mode));
        only_onboarding.overlay = Overlay::None;
        assert!(
            wants_modal(&only_onboarding),
            "онбординг сам по себе требует модалку — это первый запуск"
        );

        let plain = app_for_keys();
        assert!(
            !wants_modal(&plain),
            "без онбординга и оверлея модалка не нужна"
        );
    }

    #[test]
    fn a_key_release_is_not_a_key_press() {
        // Windows шлёт и Press, и Release. Без фильтра по kind каждая клавиша срабатывала бы
        // ДВАЖДЫ: напечатал «а» — получил «аа».
        let mut app = app_for_keys();
        apply_event(
            &mut app,
            Event::Key(KeyEvent::new_with_kind(
                KeyCode::Char('а'),
                KeyModifiers::NONE,
                KeyEventKind::Press,
            )),
        );
        assert_eq!(app.input, "а", "нажатие обязано печатать");

        apply_event(
            &mut app,
            Event::Key(KeyEvent::new_with_kind(
                KeyCode::Char('а'),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            )),
        );
        assert_eq!(app.input, "а", "отпускание клавиши НЕ печатает второй раз");
    }

    #[test]
    fn a_paste_while_search_is_open_goes_to_the_query_not_the_composer() {
        // Поиск открыт (не модалка → обрабатывается в главном цикле). Вставка обязана
        // уйти в поисковый запрос, а не молча в скрытый главный композер, откуда потом
        // всплыла бы при закрытии поиска.
        let mut app = app_for_keys();
        app.open_search();
        assert_eq!(app.overlay, Overlay::Search);

        apply_event(&mut app, Event::Paste("иголка".to_string()));

        assert_eq!(
            app.search_query, "иголка",
            "вставка ушла в поисковый запрос"
        );
        assert!(
            app.input.is_empty(),
            "и НЕ в главный композер (раньше уходила туда молча)"
        );
    }

    #[test]
    fn a_paste_lands_in_the_input_whole_and_is_not_split_into_sends() {
        // Вставка с переносами — не серия нажатий Enter. Иначе каждый перенос улетел бы
        // отправкой, и вместо одного сообщения ушло бы три.
        let mut app = app_for_keys();
        apply_event(&mut app, Event::Paste("первая\nвторая\nтретья".to_string()));

        assert!(
            app.input.contains("первая"),
            "вставка не дошла: {:?}",
            app.input
        );
        assert!(
            app.input.contains("третья"),
            "вставка обрезана: {:?}",
            app.input
        );
        assert!(
            app.pending_messages.is_empty(),
            "переносы во вставке НЕ должны отправлять сообщения: {:?}",
            app.pending_messages
        );
    }

    #[test]
    fn a_resize_forces_a_full_redraw() {
        // После сна ПК или смены монитора терминал переливает содержимое, и кэш позиций живого
        // блока устаревает. Без полной перерисовки экран остаётся с обрывками.
        let mut app = app_for_keys();
        app.pending_full_redraw = false;
        apply_event(&mut app, Event::Resize(100, 40));
        assert!(
            app.pending_full_redraw,
            "ресайз обязан требовать перерисовку"
        );
    }

    #[test]
    fn an_unhandled_event_changes_nothing() {
        let mut app = app_for_keys();
        app.pending_full_redraw = false;
        apply_event(&mut app, Event::FocusGained);
        assert!(app.input.is_empty());
        assert!(!app.pending_full_redraw);
    }

    #[test]
    fn key_press_lets_through_a_press_and_stops_a_release() {
        // Общий фильтр основной петли и модалки. Разъедься они — на Windows клавиши начали бы
        // срабатывать дважды в одном месте и нормально в другом.
        let press =
            KeyEvent::new_with_kind(KeyCode::Enter, KeyModifiers::NONE, KeyEventKind::Press);
        let release =
            KeyEvent::new_with_kind(KeyCode::Enter, KeyModifiers::NONE, KeyEventKind::Release);

        assert_eq!(key_press(Event::Key(press)), Some(press));
        assert_eq!(
            key_press(Event::Key(release)),
            None,
            "отпускание — не нажатие"
        );
        assert_eq!(key_press(Event::Resize(80, 24)), None);
        assert_eq!(key_press(Event::Paste("текст".to_string())), None);
    }

    // Модалка (effort/settings/chats/онбординг) обрабатывает ТОЛЬКО клавиши. Я сам чуть не завёз
    // тут регрессию: подсунул модалке общий `apply_event` ради красоты, и вставка начала бы молча
    // копиться в буфере главного экрана, чтобы вывалиться при её закрытии. Тест стережёт границу.
    #[test]
    fn a_paste_never_leaks_into_the_input_from_behind_a_modal() {
        let mut app = app_for_keys();
        app.overlay = Overlay::Settings;
        assert!(wants_modal(&app), "экран настроек — модалка");

        // То, что делает модалка со своим событием.
        let pasted = Event::Paste("сюда нельзя".to_string());
        assert!(
            key_press(pasted).is_none(),
            "модалка видит во вставке НЕ клавишу — значит, в инпут она не уйдёт"
        );
        assert!(
            app.input.is_empty(),
            "инпут главного экрана обязан остаться пустым: {:?}",
            app.input
        );
    }

    // ─────────────────────────── ДИСПЕТЧЕР КЛАВИШ ───────────────────────────

    #[test]
    fn handle_key_routes_to_the_screen_that_is_open() {
        // Весь диспетчер клавиатуры можно было заменить пустышкой, и ни один тест не замечал:
        // пользователь жмёт клавиши, не происходит НИЧЕГО, а CI зелёный.
        let mut typing = app_for_keys();
        handle_key(&mut typing, key(KeyCode::Char('ф')));
        assert_eq!(typing.input, "ф", "без оверлея клавиша идёт в инпут");

        let mut on_effort = app_for_keys();
        on_effort.overlay = Overlay::Effort;
        handle_key(&mut on_effort, key(KeyCode::Char('ф')));
        assert!(
            on_effort.input.is_empty(),
            "на экране effort буква НЕ должна проваливаться в инпут — иначе экраны перепутаны"
        );

        let mut on_effort_esc = app_for_keys();
        on_effort_esc.overlay = Overlay::Effort;
        handle_key(&mut on_effort_esc, key(KeyCode::Esc));
        assert_eq!(
            on_effort_esc.overlay,
            Overlay::None,
            "Esc на экране effort обязан его закрыть — значит клавиша дошла до нужного обработчика"
        );
    }

    // ─────────────────────────── РАЗБОР АРГУМЕНТОВ ───────────────────────────

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn launch_for_sends_each_form_of_the_command_line_where_it_belongs() {
        assert_eq!(launch_for(&argv(&[])), Launch::Tui);
        assert_eq!(launch_for(&argv(&["-h"])), Launch::Usage);
        assert_eq!(launch_for(&argv(&["--help"])), Launch::Usage);
        assert_eq!(launch_for(&argv(&["-V"])), Launch::Version);
        assert_eq!(launch_for(&argv(&["--version"])), Launch::Version);
        assert_eq!(launch_for(&argv(&["напиши тест"])), Launch::Engine);
        // --resume / -r открывают TUI на списке сохранённых чатов (иначе флаг уехал бы в
        // UnknownFlag — resume был только командой /resume внутри TUI).
        assert_eq!(launch_for(&argv(&["--resume"])), Launch::ResumeTui);
        assert_eq!(launch_for(&argv(&["-r"])), Launch::ResumeTui);

        // Справка перекрывает всё: `clave задача --help` — это просьба о справке.
        assert_eq!(launch_for(&argv(&["задача", "--help"])), Launch::Usage);
        // ...а версия перекрывает запуск задачи, но не справку.
        assert_eq!(launch_for(&argv(&["задача", "-V"])), Launch::Version);
    }

    #[test]
    fn an_unknown_flag_never_becomes_a_task() {
        // Ключевое: неизвестный флаг НЕ должен молча уехать в движок «задачей» — это запустило
        // бы платный цикл планирования там, где человек просто опечатался.
        assert_eq!(
            launch_for(&argv(&["--serve"])),
            Launch::UnknownFlag("--serve".to_string())
        );
        assert_eq!(
            launch_for(&argv(&["--serve", "8080"])),
            Launch::UnknownFlag("--serve".to_string())
        );
        // А вот дефис ВНУТРИ задачи — это просто текст, а не флаг.
        assert_eq!(
            launch_for(&argv(&["почини", "--flag", "в", "коде"])),
            Launch::Engine
        );
    }

    #[test]
    fn usage_text_names_the_command_and_the_ways_to_run_it() {
        // print_usage целиком заменялась пустышкой: `clave --help` мог печатать пустоту.
        let text = usage_text();
        assert!(
            text.contains(APP_COMMAND),
            "справка обязана назвать команду"
        );
        assert!(text.contains("--help"), "справка обязана упомянуть --help");
        assert!(
            text.contains("--resume"),
            "справка обязана упомянуть --resume"
        );
        assert!(
            text.contains("<task...>"),
            "справка обязана показать запуск задачи"
        );
        assert!(text.contains("Open TUI"), "справка обязана упомянуть TUI");
    }

    // ─────────────────── хелперы экранов и оверлеев ───────────────────

    /// `Onboarding::new` зондирует codex/claude настоящими процессами CLI — в юнит-тесте
    /// это флейк, а в CI реальный запуск. Поэтому состояние экрана собираем полями.
    fn onboarding_at(step: OnboardingStep, provider_index: usize) -> Onboarding {
        Onboarding {
            step,
            provider_index,
            setting_index: 0,
            codex_installed: true,
            claude_installed: true,
            codex_authenticated: true,
            claude_authenticated: true,
            codex_status: String::new(),
            claude_status: String::new(),
            message: String::new(),
        }
    }

    fn app_with_onboarding(step: OnboardingStep, provider_index: usize) -> App {
        let mut app = app_for_keys();
        app.set_mode(provider_mode(provider_index));
        app.rounds = 5;
        app.onboarding = Some(onboarding_at(step, provider_index));
        app
    }

    fn onboarding_of(app: &App) -> &Onboarding {
        app.onboarding.as_ref().expect("онбординг открыт")
    }

    /// Режим задаём явно: от него зависят и число строк пикера, и что двигают ←/→.
    fn app_for_effort(mode: Mode) -> App {
        let mut app = app_for_keys();
        app.overlay = Overlay::Effort;
        app.mode = mode;
        app.linked_effort_split = false;
        app.effort_focus = 0;
        app.effort_index = effort_index_for("high");
        app.codex_effort_index = effort_index_for("high");
        app.claude_effort_index = effort_index_for("high");
        app.effort_original = Some(app.effort_snapshot());
        app
    }

    fn app_for_settings() -> App {
        let mut app = app_for_keys();
        app.overlay = Overlay::Settings;
        app.settings_focus = 0;
        app.rounds = 5;
        app.theme = Theme::Purple;
        app.settings_original = Some(app.settings_snapshot());
        app
    }

    fn app_for_chats() -> App {
        let mut app = app_for_keys();
        app.overlay = Overlay::Chats;
        app.chats_index = 0;
        app.chats_picker = ["chat-one", "chat-two"]
            .iter()
            .map(|id| ChatSummary {
                id: (*id).to_string(),
                title: (*id).to_string(),
                lines: 3,
                modified: SystemTime::UNIX_EPOCH,
            })
            .collect();
        app
    }

    fn ask_question(question: &str, multi: bool, labels: &[&str]) -> AskQuestion {
        AskQuestion {
            question: question.to_string(),
            multi,
            options: labels
                .iter()
                .map(|label| AskOption {
                    label: (*label).to_string(),
                    note: None,
                })
                .collect(),
            allow_custom: true,
        }
    }

    /// running = true: `ask_submit` уходит в `start_chat`, который в этом состоянии кладёт
    /// сообщение в очередь и НЕ поднимает провайдер.
    fn app_with_ask(questions: Vec<AskQuestion>) -> App {
        let mut app = app_for_keys();
        app.running = true;
        app.ask_prompt_pending = Some(AskPrompt { questions });
        app.open_pending_ask();
        app
    }

    fn ask_state(app: &App) -> &AskState {
        app.ask.as_ref().expect("селектор открыт")
    }

    fn transcript_has(app: &App, needle: &str) -> bool {
        app.transcript.iter().any(|line| line.contains(needle))
    }

    // ───────────────────────── handle_effort_key ─────────────────────────

    #[test]
    fn effort_down_stops_at_last_row() {
        // ClaudeCodex без раздельного усилия — ровно две строки пикера.
        let mut app = app_for_effort(Mode::ClaudeCodex);
        handle_effort_key(&mut app, key(KeyCode::Down));
        assert_eq!(app.effort_focus, 1, "↓ переводит фокус на вторую строку");
        handle_effort_key(&mut app, key(KeyCode::Down));
        assert_eq!(app.effort_focus, 1, "ниже последней строки фокус не уходит");
    }

    #[test]
    fn effort_up_moves_focus_back() {
        let mut app = app_for_effort(Mode::ClaudeCodex);
        app.effort_focus = 1;
        handle_effort_key(&mut app, key(KeyCode::Up));
        assert_eq!(app.effort_focus, 0);
        handle_effort_key(&mut app, key(KeyCode::Up));
        assert_eq!(app.effort_focus, 0, "выше первой строки фокус не уходит");
    }

    #[test]
    fn effort_left_and_right_change_effort() {
        let mut down = app_for_effort(Mode::ClaudeOnly);
        handle_effort_key(&mut down, key(KeyCode::Left));
        assert_eq!(
            effort_label(down.claude_effort_index),
            "medium",
            "← ослабляет"
        );

        let mut up = app_for_effort(Mode::ClaudeOnly);
        handle_effort_key(&mut up, key(KeyCode::Right));
        assert_eq!(effort_label(up.claude_effort_index), "max", "→ усиливает");
    }

    #[test]
    fn effort_enter_saves_and_closes() {
        let mut app = app_for_effort(Mode::ClaudeOnly);
        handle_effort_key(&mut app, key(KeyCode::Right));
        handle_effort_key(&mut app, key(KeyCode::Enter));
        assert_eq!(app.overlay, Overlay::None);
        assert!(
            app.effort_original.is_none(),
            "снимок отпущен — правка принята"
        );
        assert_eq!(app.status, "готов");
        assert_eq!(
            effort_label(app.claude_effort_index),
            "max",
            "Enter не откатывает"
        );
        assert!(
            transcript_has(&app, "Set to"),
            "лента: {:?}",
            app.transcript
        );
        assert!(app.config_path.exists(), "Enter сохраняет конфиг");
    }

    #[test]
    fn effort_esc_restores_snapshot() {
        let mut app = app_for_effort(Mode::ClaudeOnly);
        app.claude_effort_index = effort_index_for("low");
        handle_effort_key(&mut app, key(KeyCode::Esc));
        assert_eq!(
            effort_label(app.claude_effort_index),
            "high",
            "Esc возвращает усилие из снимка"
        );
        assert!(app.effort_original.is_none());
        assert_eq!(app.overlay, Overlay::None);
        assert_eq!(app.status, "готов");
        assert!(
            transcript_has(&app, "Cancelled"),
            "лента: {:?}",
            app.transcript
        );
    }

    #[test]
    fn effort_quits_only_on_double_ctrl_c() {
        let mut app = app_for_effort(Mode::ClaudeOnly);
        handle_effort_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(!app.should_quit, "одиночный Ctrl+C не выходит");
        handle_effort_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(app.should_quit, "двойной Ctrl+C выходит");

        let mut plain = app_for_effort(Mode::ClaudeOnly);
        handle_effort_key(&mut plain, key(KeyCode::Char('c')));
        handle_effort_key(&mut plain, key(KeyCode::Char('c')));
        assert!(!plain.should_quit, "простая «c» — не Ctrl+C");
        assert!(
            plain.last_ctrl_c_at.is_none(),
            "простая «c» не считается за Ctrl+C"
        );
    }

    // ─────────────────── handle_onboarding_settings_key ───────────────────

    #[test]
    fn onboarding_settings_down_stops_at_last_row() {
        let mut app = app_with_onboarding(OnboardingStep::Settings, 0);
        handle_onboarding_settings_key(&mut app, key(KeyCode::Down));
        assert_eq!(onboarding_of(&app).setting_index, 1);
        handle_onboarding_settings_key(&mut app, key(KeyCode::Down));
        assert_eq!(onboarding_of(&app).setting_index, 2);
        handle_onboarding_settings_key(&mut app, key(KeyCode::Down));
        assert_eq!(
            onboarding_of(&app).setting_index,
            2,
            "ниже третьей строки не уходит"
        );
    }

    #[test]
    fn onboarding_settings_up_moves_back() {
        let mut app = app_with_onboarding(OnboardingStep::Settings, 0);
        app.onboarding.as_mut().expect("онбординг").setting_index = 2;
        handle_onboarding_settings_key(&mut app, key(KeyCode::Up));
        assert_eq!(onboarding_of(&app).setting_index, 1);
    }

    #[test]
    fn onboarding_settings_left_right_change_rounds() {
        let mut less = app_with_onboarding(OnboardingStep::Settings, 0);
        handle_onboarding_settings_key(&mut less, key(KeyCode::Left));
        assert_eq!(less.rounds, 4, "← уменьшает раунды");

        let mut more = app_with_onboarding(OnboardingStep::Settings, 0);
        handle_onboarding_settings_key(&mut more, key(KeyCode::Right));
        assert_eq!(more.rounds, 6, "→ увеличивает раунды");
    }

    #[test]
    fn onboarding_settings_l_toggles_language() {
        let mut app = app_with_onboarding(OnboardingStep::Settings, 0);
        handle_onboarding_settings_key(&mut app, key(KeyCode::Char('l')));
        assert_eq!(app.lang, Language::En);

        let mut upper = app_with_onboarding(OnboardingStep::Settings, 0);
        upper.lang = Language::En;
        handle_onboarding_settings_key(&mut upper, key(KeyCode::Char('L')));
        assert_eq!(
            upper.lang,
            Language::Ru,
            "переключатель работает в обе стороны"
        );
    }

    #[test]
    fn onboarding_settings_enter_finishes_onboarding() {
        let mut app = app_with_onboarding(OnboardingStep::Settings, 0);
        handle_onboarding_settings_key(&mut app, key(KeyCode::Enter));
        assert!(app.onboarding.is_none(), "Enter закрывает онбординг");
        assert_eq!(app.status, "готов");
        assert!(app.config_path.exists(), "Enter сохраняет конфиг");
    }

    #[test]
    fn onboarding_settings_backspace_and_esc_return_to_auth() {
        for code in [KeyCode::Backspace, KeyCode::Esc] {
            let mut app = app_with_onboarding(OnboardingStep::Settings, 0);
            handle_onboarding_settings_key(&mut app, key(code));
            assert_eq!(
                onboarding_of(&app).step,
                OnboardingStep::Auth,
                "{code:?} возвращает на шаг авторизации"
            );
        }
    }

    // ───────────────────────── handle_ask_key ─────────────────────────

    #[test]
    fn ask_down_and_up_wrap_over_rows() {
        let mut app = app_with_ask(vec![ask_question(
            "Продолжить?",
            false,
            &["Да", "Нет", "Позже"],
        )]);
        handle_ask_key(&mut app, key(KeyCode::Down));
        assert_eq!(ask_state(&app).answers[0].cursor, 1, "↓ идёт вниз");

        let mut up = app_with_ask(vec![ask_question(
            "Продолжить?",
            false,
            &["Да", "Нет", "Позже"],
        )]);
        handle_ask_key(&mut up, key(KeyCode::Up));
        assert_eq!(
            ask_state(&up).answers[0].cursor,
            3,
            "↑ с первой строки заворачивает на «свой ответ»"
        );
    }

    #[test]
    fn ask_tab_and_right_go_to_next_question() {
        for code in [KeyCode::Tab, KeyCode::Right] {
            let mut app = app_with_ask(vec![
                ask_question("Первый?", false, &["Да"]),
                ask_question("Второй?", false, &["Да"]),
            ]);
            handle_ask_key(&mut app, key(code));
            assert_eq!(
                ask_state(&app).step,
                1,
                "{code:?} ведёт к следующему вопросу"
            );
        }
    }

    #[test]
    fn ask_backtab_and_left_go_back() {
        for code in [KeyCode::BackTab, KeyCode::Left] {
            let mut app = app_with_ask(vec![
                ask_question("Первый?", false, &["Да"]),
                ask_question("Второй?", false, &["Да"]),
            ]);
            handle_ask_key(&mut app, key(KeyCode::Tab));
            handle_ask_key(&mut app, key(code));
            assert_eq!(
                ask_state(&app).step,
                0,
                "{code:?} возвращает к прошлому вопросу"
            );
        }
    }

    #[test]
    fn ask_enter_submits_single_question() {
        let mut app = app_with_ask(vec![ask_question("Продолжить?", false, &["Да", "Нет"])]);
        handle_ask_key(&mut app, key(KeyCode::Enter));
        assert!(app.ask.is_none(), "Enter закрывает селектор");
        let queued = app.pending_messages.front().expect("сообщение в очереди");
        assert!(
            queued.contains("«Да»"),
            "отправлен выбранный вариант: {queued}"
        );
    }

    #[test]
    fn ask_esc_closes_without_sending() {
        let mut app = app_with_ask(vec![ask_question("Продолжить?", false, &["Да", "Нет"])]);
        handle_ask_key(&mut app, key(KeyCode::Esc));
        assert!(app.ask.is_none());
        assert_eq!(app.status, "закрыто");
        assert!(app.pending_messages.is_empty(), "Esc ничего не отправляет");
    }

    #[test]
    fn ask_backspace_edits_custom_answer() {
        let mut app = app_with_ask(vec![ask_question("Продолжить?", false, &["Да", "Нет"])]);
        {
            let state = app.ask.as_mut().expect("селектор открыт");
            state.answers[0].cursor = 2; // строка «свой ответ»
            state.answers[0].custom = "ab".to_string();
        }
        handle_ask_key(&mut app, key(KeyCode::Backspace));
        assert_eq!(ask_state(&app).answers[0].custom, "a");
    }

    #[test]
    fn ask_plain_char_types_into_custom_answer() {
        // Именно 'c': мутант `&&` → `||` увёл бы её в ветку Ctrl+C с ранним return.
        let mut app = app_with_ask(vec![ask_question("Продолжить?", false, &["Да", "Нет"])]);
        app.ask.as_mut().expect("селектор открыт").answers[0].cursor = 2;
        handle_ask_key(&mut app, key(KeyCode::Char('c')));
        assert_eq!(ask_state(&app).answers[0].custom, "c");
        assert!(app.last_ctrl_c_at.is_none(), "простая «c» — не Ctrl+C");
    }

    #[test]
    fn ask_space_toggles_option_but_other_chars_do_not() {
        let mut space = app_with_ask(vec![ask_question("Что взять?", true, &["Да", "Нет"])]);
        handle_ask_key(&mut space, key(KeyCode::Char(' ')));
        assert!(
            ask_state(&space).answers[0].checked[0],
            "Space отмечает вариант"
        );

        let mut other = app_with_ask(vec![ask_question("Что взять?", true, &["Да", "Нет"])]);
        handle_ask_key(&mut other, key(KeyCode::Char('x')));
        assert!(
            !ask_state(&other).answers[0].checked[0],
            "любой другой символ на варианте отметку не ставит"
        );
    }

    #[test]
    fn ask_quits_on_double_ctrl_c() {
        let mut app = app_with_ask(vec![ask_question("Продолжить?", false, &["Да"])]);
        handle_ask_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(!app.should_quit);
        handle_ask_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(app.should_quit, "двойной Ctrl+C выходит");
    }

    // ───────────────────────── handle_settings_key ─────────────────────────

    #[test]
    fn settings_up_and_down_move_focus() {
        let mut app = app_for_settings();
        handle_settings_key(&mut app, key(KeyCode::Down));
        assert_eq!(app.settings_focus, 1, "↓ идёт вниз по строкам");

        app.settings_focus = 3;
        handle_settings_key(&mut app, key(KeyCode::Up));
        assert_eq!(app.settings_focus, 2, "↑ идёт вверх");
    }

    #[test]
    fn settings_left_and_right_change_rounds() {
        let mut less = app_for_settings();
        less.settings_focus = 4; // строка раундов
        handle_settings_key(&mut less, key(KeyCode::Left));
        assert_eq!(less.rounds, 4, "← уменьшает раунды");

        let mut more = app_for_settings();
        more.settings_focus = 4;
        handle_settings_key(&mut more, key(KeyCode::Right));
        assert_eq!(more.rounds, 6, "→ увеличивает раунды");
    }

    #[test]
    fn settings_enter_saves_and_closes() {
        let mut app = app_for_settings();
        app.settings_focus = 4;
        handle_settings_key(&mut app, key(KeyCode::Right));
        handle_settings_key(&mut app, key(KeyCode::Enter));
        assert_eq!(app.overlay, Overlay::None);
        assert!(
            app.settings_original.is_none(),
            "снимок отпущен — правка принята"
        );
        assert_eq!(app.rounds, 6, "Enter не откатывает значение");
        assert_eq!(app.status, "готов");
        assert!(transcript_has(&app, "Saved"), "лента: {:?}", app.transcript);
        assert!(app.config_path.exists(), "Enter сохраняет конфиг");
    }

    #[test]
    fn settings_esc_restores_snapshot() {
        let mut app = app_for_settings();
        app.theme = Theme::Amber;
        app.rounds = 9;
        handle_settings_key(&mut app, key(KeyCode::Esc));
        assert_eq!(app.theme, Theme::Purple, "Esc возвращает тему из снимка");
        assert_eq!(app.rounds, 5, "Esc возвращает раунды из снимка");
        assert!(app.settings_original.is_none());
        assert_eq!(app.overlay, Overlay::None);
        assert_eq!(app.status, "готов");
        assert!(
            transcript_has(&app, "Cancelled"),
            "лента: {:?}",
            app.transcript
        );
    }

    #[test]
    fn settings_quits_only_on_double_ctrl_c() {
        let mut app = app_for_settings();
        handle_settings_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(!app.should_quit, "одиночный Ctrl+C не выходит");
        handle_settings_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(app.should_quit, "двойной Ctrl+C выходит");

        let mut plain = app_for_settings();
        handle_settings_key(&mut plain, key(KeyCode::Char('c')));
        handle_settings_key(&mut plain, key(KeyCode::Char('c')));
        assert!(!plain.should_quit, "простая «c» — не Ctrl+C");
        assert!(plain.last_ctrl_c_at.is_none());
    }

    // ─────────────────── adjust_onboarding_setting ───────────────────

    #[test]
    fn adjust_onboarding_rounds_by_direction() {
        let mut back = app_with_onboarding(OnboardingStep::Settings, 0);
        adjust_onboarding_setting(&mut back, 0, -1);
        assert_eq!(back.rounds, 4, "отрицательное направление уменьшает");

        let mut forward = app_with_onboarding(OnboardingStep::Settings, 0);
        adjust_onboarding_setting(&mut forward, 0, 1);
        assert_eq!(forward.rounds, 6, "положительное направление увеличивает");

        // Нулевое направление — «вперёд»: единственный вход, различающий `<` и `<=`.
        let mut zero = app_with_onboarding(OnboardingStep::Settings, 0);
        adjust_onboarding_setting(&mut zero, 0, 0);
        assert_eq!(zero.rounds, 6, "направление 0 считается движением вперёд");
    }

    #[test]
    fn adjust_onboarding_rounds_are_clamped() {
        let mut low = app_with_onboarding(OnboardingStep::Settings, 0);
        low.rounds = 1;
        adjust_onboarding_setting(&mut low, 0, -1);
        assert_eq!(low.rounds, 1, "меньше одного раунда не бывает");

        let mut high = app_with_onboarding(OnboardingStep::Settings, 0);
        high.rounds = 9;
        adjust_onboarding_setting(&mut high, 0, 1);
        assert_eq!(high.rounds, 9, "больше девяти раундов не бывает");
    }

    #[test]
    fn adjust_onboarding_startup_effort() {
        let mut less = app_with_onboarding(OnboardingStep::Settings, 3); // ClaudeOnly
        less.claude_effort_index = effort_index_for("high");
        adjust_onboarding_setting(&mut less, 1, -1);
        assert_eq!(effort_label(less.claude_effort_index), "medium");

        let mut more = app_with_onboarding(OnboardingStep::Settings, 3);
        more.claude_effort_index = effort_index_for("high");
        adjust_onboarding_setting(&mut more, 1, 1);
        assert_eq!(effort_label(more.claude_effort_index), "max");
    }

    #[test]
    fn adjust_onboarding_language_toggles() {
        let mut app = app_with_onboarding(OnboardingStep::Settings, 0);
        adjust_onboarding_setting(&mut app, 2, 1);
        assert_eq!(app.lang, Language::En);
        adjust_onboarding_setting(&mut app, 2, 1);
        assert_eq!(
            app.lang,
            Language::Ru,
            "переключатель работает в обе стороны"
        );
    }

    // ───────────────────────── handle_search_key ─────────────────────────

    #[test]
    fn chats_down_stops_at_last_chat() {
        let mut app = app_for_chats();
        handle_chats_key(&mut app, key(KeyCode::Down));
        assert_eq!(app.chats_index, 1);
        handle_chats_key(&mut app, key(KeyCode::Down));
        assert_eq!(app.chats_index, 1, "ниже последнего чата не уходит");
    }

    #[test]
    fn chats_up_moves_back() {
        let mut app = app_for_chats();
        app.chats_index = 1;
        handle_chats_key(&mut app, key(KeyCode::Up));
        assert_eq!(app.chats_index, 0);
    }

    #[test]
    fn chats_enter_closes_and_resumes_selected() {
        // Файла чата нет — resume_chat отвечает «Чат не найден.». Это и доказывает,
        // что Enter действительно позвал resume_chat, ничего при этом не запуская.
        let mut app = app_for_chats();
        handle_chats_key(&mut app, key(KeyCode::Enter));
        assert_eq!(app.overlay, Overlay::None);
        assert!(
            transcript_has(&app, "Чат не найден."),
            "Enter восстанавливает выбранный чат: {:?}",
            app.transcript
        );
    }

    #[test]
    fn chats_esc_closes_without_resume() {
        let mut app = app_for_chats();
        handle_chats_key(&mut app, key(KeyCode::Esc));
        assert_eq!(app.overlay, Overlay::None);
        assert!(app.transcript.is_empty(), "Esc чат не восстанавливает");
    }

    #[test]
    fn chats_quits_only_on_double_ctrl_c() {
        let mut app = app_for_chats();
        handle_chats_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(!app.should_quit, "одиночный Ctrl+C не выходит");
        handle_chats_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(app.should_quit, "двойной Ctrl+C выходит");

        let mut plain = app_for_chats();
        handle_chats_key(&mut plain, key(KeyCode::Char('c')));
        handle_chats_key(&mut plain, key(KeyCode::Char('c')));
        assert!(!plain.should_quit, "простая «c» — не Ctrl+C");
        assert!(plain.last_ctrl_c_at.is_none());
    }

    // ─────────────────── handle_onboarding_provider_key ───────────────────

    #[test]
    fn onboarding_provider_down_selects_next_and_stops_at_last() {
        let mut app = app_with_onboarding(OnboardingStep::Provider, 0);
        handle_onboarding_provider_key(&mut app, key(KeyCode::Down));
        assert_eq!(onboarding_of(&app).provider_index, 1);
        assert_eq!(app.mode, Mode::ClaudeCodex, "режим следует за выбором");

        let mut last = app_with_onboarding(OnboardingStep::Provider, 3);
        handle_onboarding_provider_key(&mut last, key(KeyCode::Down));
        assert_eq!(
            onboarding_of(&last).provider_index,
            3,
            "ниже последнего провайдера не уходит"
        );
        assert_eq!(last.mode, Mode::ClaudeOnly);
    }

    #[test]
    fn onboarding_provider_up_selects_previous() {
        let mut app = app_with_onboarding(OnboardingStep::Provider, 2);
        handle_onboarding_provider_key(&mut app, key(KeyCode::Up));
        assert_eq!(onboarding_of(&app).provider_index, 1);
        assert_eq!(app.mode, Mode::ClaudeCodex);
    }

    #[test]
    fn onboarding_provider_enter_goes_to_auth() {
        let mut app = app_with_onboarding(OnboardingStep::Provider, 3);
        handle_onboarding_provider_key(&mut app, key(KeyCode::Enter));
        assert_eq!(
            app.mode,
            Mode::ClaudeOnly,
            "Enter фиксирует выбранный режим"
        );
        let onboarding = onboarding_of(&app);
        assert_eq!(onboarding.step, OnboardingStep::Auth);
        assert!(
            onboarding.message.contains("авторизацию"),
            "подсказка шага авторизации: {}",
            onboarding.message
        );
    }

    // ─────────────────── handle_onboarding_auth_key ───────────────────

    #[test]
    fn onboarding_auth_c_prepares_codex_login() {
        // Команда только кладётся в поле; запускает её позже сам runtime — тест ничего не спавнит.
        for code in [KeyCode::Char('c'), KeyCode::Char('C')] {
            let mut app = app_with_onboarding(OnboardingStep::Auth, 0);
            handle_onboarding_auth_key(&mut app, key(code));
            let command = app.pending_external.as_ref().expect("команда логина");
            assert_eq!(command.program, "codex");
            assert_eq!(command.args, &["login"]);
        }
    }

    #[test]
    fn onboarding_auth_l_prepares_claude_login() {
        for code in [KeyCode::Char('l'), KeyCode::Char('L')] {
            let mut app = app_with_onboarding(OnboardingStep::Auth, 0);
            handle_onboarding_auth_key(&mut app, key(code));
            let command = app.pending_external.as_ref().expect("команда логина");
            assert_eq!(command.program, "claude");
            assert_eq!(command.args, &["auth", "login"]);
        }
    }

    #[test]
    fn onboarding_auth_enter_goes_to_settings() {
        let mut app = app_with_onboarding(OnboardingStep::Auth, 0);
        handle_onboarding_auth_key(&mut app, key(KeyCode::Enter));
        let onboarding = onboarding_of(&app);
        assert_eq!(onboarding.step, OnboardingStep::Settings);
        assert!(
            !onboarding.message.is_empty(),
            "шаг настроек объясняет себя"
        );
    }

    #[test]
    fn onboarding_auth_backspace_and_esc_return_to_provider() {
        for code in [KeyCode::Backspace, KeyCode::Esc] {
            let mut app = app_with_onboarding(OnboardingStep::Auth, 0);
            handle_onboarding_auth_key(&mut app, key(code));
            assert_eq!(
                onboarding_of(&app).step,
                OnboardingStep::Provider,
                "{code:?} возвращает к выбору провайдера"
            );
        }
    }

    // ───────────────────────── handle_shortcuts_key ─────────────────────────

    #[test]
    fn onboarding_dispatches_plain_key_to_current_step() {
        let mut app = app_with_onboarding(OnboardingStep::Auth, 0);
        handle_onboarding_key(&mut app, key(KeyCode::Char('c')));
        let command = app
            .pending_external
            .as_ref()
            .expect("клавиша дошла до шага авторизации");
        assert_eq!(command.program, "codex");
        assert!(app.last_ctrl_c_at.is_none(), "простая «c» — не Ctrl+C");
    }

    #[test]
    fn onboarding_quits_on_double_ctrl_c() {
        let mut app = app_with_onboarding(OnboardingStep::Provider, 1);
        handle_onboarding_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(!app.should_quit);
        handle_onboarding_key(&mut app, ctrl(KeyCode::Char('c')));
        assert!(app.should_quit, "двойной Ctrl+C выходит");
        assert_eq!(
            onboarding_of(&app).provider_index,
            1,
            "Ctrl+C до навигации по шагу не доходит"
        );
    }

    // ───────────────────────── handle_marketplace_key ─────────────────────────

    fn market(provider: Provider, name: &str) -> Marketplace {
        Marketplace {
            provider,
            name: name.to_string(),
            source: format!("src/{name}"),
        }
    }

    /// Tab листает табы по кругу, Shift+Tab — назад; панель при этом не закрывается.
    #[test]
    fn tab_cycles_through_tabs() {
        let mut app = app_for_keys();
        app.overlay = Overlay::Plugins;
        app.plugins_tab = PluginsTab::Overview;

        handle_plugins_key(&mut app, key(KeyCode::Tab));
        assert_eq!(app.plugins_tab, PluginsTab::Installed);
        handle_plugins_key(&mut app, key(KeyCode::Tab));
        assert_eq!(app.plugins_tab, PluginsTab::Catalog);
        handle_plugins_key(&mut app, key(KeyCode::Tab));
        assert_eq!(app.plugins_tab, PluginsTab::Sources);
        handle_plugins_key(&mut app, key(KeyCode::Tab));
        assert_eq!(app.plugins_tab, PluginsTab::Overview, "по кругу");
        assert_eq!(app.overlay, Overlay::Plugins, "панель осталась открытой");

        handle_plugins_key(&mut app, key(KeyCode::BackTab));
        assert_eq!(app.plugins_tab, PluginsTab::Sources, "Shift+Tab — назад");
    }

    /// ←/→ в списковом табе переключают провайдера (иначе codex тонет под каталогом claude).
    #[test]
    fn left_right_toggles_provider_in_list_tabs() {
        let mut app = app_for_keys();
        app.overlay = Overlay::Plugins;
        app.plugins_tab = PluginsTab::Catalog;
        assert_eq!(
            app.plugins_provider,
            Provider::Claude,
            "по умолчанию Claude"
        );

        handle_plugins_key(&mut app, key(KeyCode::Right));
        assert_eq!(
            app.plugins_provider,
            Provider::Codex,
            "→ переключил на Codex"
        );
        handle_plugins_key(&mut app, key(KeyCode::Left));
        assert_eq!(app.plugins_provider, Provider::Claude, "← вернул Claude");
    }

    /// `a` открывает ввод адреса; печать его наполняет; Tab меняет провайдера (а не выходит);
    /// Backspace стирает; Esc закрывает ввод, оставаясь в режиме источников.
    #[test]
    fn marketplace_add_input_types_toggles_provider_and_cancels() {
        let mut app = app_for_keys();
        app.overlay = Overlay::Plugins;
        app.plugins_tab = PluginsTab::Sources;
        app.marketplaces = vec![market(Provider::Claude, "official")];
        app.marketplaces_index = 0;

        handle_plugins_key(&mut app, key(KeyCode::Char('a')));
        let add = app.marketplace_input.as_ref().expect("a открыл ввод");
        assert_eq!(
            add.provider,
            Provider::Claude,
            "цель — провайдер выбранного"
        );

        handle_plugins_key(&mut app, key(KeyCode::Char('x')));
        handle_plugins_key(&mut app, key(KeyCode::Char('y')));
        assert_eq!(
            app.marketplace_input.as_ref().unwrap().source,
            "xy",
            "печать наполнила адрес"
        );

        handle_plugins_key(&mut app, key(KeyCode::Tab));
        assert_eq!(
            app.marketplace_input.as_ref().unwrap().provider,
            Provider::Codex,
            "Tab в вводе сменил провайдера, а не вышел из режима"
        );

        handle_plugins_key(&mut app, key(KeyCode::Backspace));
        assert_eq!(app.marketplace_input.as_ref().unwrap().source, "x");

        handle_plugins_key(&mut app, key(KeyCode::Esc));
        assert!(app.marketplace_input.is_none(), "Esc закрыл ввод");
        assert_eq!(app.plugins_tab, PluginsTab::Sources, "но из таба не вышел");
    }

    /// Enter на источнике просит подтверждения удаления; Esc отменяет его, следующий Esc
    /// (уже без подтверждения) закрывает панель.
    #[test]
    fn marketplace_enter_confirms_remove_then_esc_cancels_and_closes() {
        let mut app = app_for_keys();
        app.overlay = Overlay::Plugins;
        app.plugins_tab = PluginsTab::Sources;
        app.run_hooks.spawn = |_tx, _body| {};
        app.marketplaces = vec![market(Provider::Codex, "openai-bundled")];
        app.marketplaces_index = 0;

        handle_plugins_key(&mut app, key(KeyCode::Enter));
        let confirm = app
            .marketplace_confirm
            .as_ref()
            .expect("Enter → подтверждение");
        assert_eq!(confirm.name, "openai-bundled");

        handle_plugins_key(&mut app, key(KeyCode::Esc));
        assert!(
            app.marketplace_confirm.is_none(),
            "Esc отменил подтверждение"
        );
        assert_eq!(
            app.plugins_tab,
            PluginsTab::Sources,
            "остались в источниках"
        );

        handle_plugins_key(&mut app, key(KeyCode::Esc));
        assert_eq!(app.overlay, Overlay::None, "второй Esc закрыл панель");
    }
}
