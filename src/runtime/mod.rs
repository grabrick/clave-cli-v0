use crate::prelude::*;
use crate::*;

mod dialogs;
mod input;
mod onboarding;
mod plugins;
mod terminal;
mod welcome;
pub(crate) use dialogs::*;
pub(crate) use input::*;
pub(crate) use onboarding::*;
pub(crate) use plugins::*;
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
}
