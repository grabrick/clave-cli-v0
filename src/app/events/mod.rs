use super::*;

mod activity;
mod reveal;
pub(crate) use reveal::*;

#[derive(Debug)]
pub(crate) enum WorkerEvent {
    Line(String),
    ChatLine(String),
    /// Инкремент ответа модели (токен-стрим) — показывается вживую до завершения.
    StreamDelta(String),
    /// Инкремент рассуждения (extended thinking) — стримится в лоадер до ответа.
    ReasoningDelta(String),
    Activity(String),
    Done(i32),
    ChatDone(Provider, i32, Option<RunUsage>),
    PlanReady(Provider, String, i32, Option<RunUsage>),
    Cancelled,
    Failed(String),
    /// Провайдер не залогинен — проверка ушла в воркер, чтобы не морозить UI.
    AuthMissing(Provider),
    /// Список плагинов codex догружен воркером (claude читается синхронно при открытии панели).
    PluginsLoaded(Vec<PluginEntry>),
    /// Действие над плагином (install/uninstall/enable/…) завершено — обновляем список.
    PluginActionDone,
    /// Список marketplace-источников codex догружен воркером (claude читается синхронно).
    MarketplacesLoaded(Vec<Marketplace>),
    /// Добавление/удаление источника завершено — перезагружаем список источников.
    MarketplaceActionDone,
    /// Тандем не достиг консенсуса: воркер заблокирован и ждёт решения пользователя
    /// (исполнить последнюю версию / отменить). Открывает гейт `tandem_gate`.
    TandemNeedsApproval,
    /// Исполнителю не хватает данных: воркер заблокирован и ждёт ответ пользователя
    /// (вопросы уже показаны). `Some` — закрытый вопрос, открываем селектор выбора;
    /// `None` — открытый вопрос, текстовый ввод-гейт.
    TandemNeedsInput(Option<AskPrompt>),
    /// Шаг тандема (или его статус) полностью выведен — фиксируем его в ленте СРАЗУ и гасим
    /// живой стрим. Иначе шаги копились бы в буфере до гейта/конца, а `⏺`-стрим двоил бы их.
    TandemStepEnd,
}

pub(crate) enum ChatRunResult {
    Completed(i32, String, String, Option<RunUsage>),
    Cancelled,
}

/// Человеческий финал прогона вместо машинного «Clave завершился с кодом N» — сырой код
/// в ленте читается как падение, даже когда всё хорошо.
///
/// Для `/dev` код выхода — это ИСХОД, а не поломка: 0 — сошлось, 1 — не сошлось (агент не
/// внёс правок либо не довели до зелёного), 2 — рабочее дерево не чистое. Единицу нельзя
/// звать «ошибкой»: ошибки не было.
///
/// Возвращает (статус для футера, строку в ленту).
pub(crate) fn run_finish_lines(dev: bool, code: i32, lang: Language) -> (String, String) {
    if dev {
        return match code {
            0 => (
                lang.choose("готово", "done").to_string(),
                lang.choose(
                    "⏺ Самопиление завершено: правки лежат в отдельном worktree и ждут ревью.",
                    "⏺ Self-dev finished: changes are in a separate worktree, awaiting review.",
                )
                .to_string(),
            ),
            1 => (
                lang.choose("не сошлось", "not converged").to_string(),
                lang.choose(
                    "⏺ Самопиление завершено: до зелёного не довели — смотри отчёт выше.",
                    "⏺ Self-dev finished: did not converge — see the report above.",
                )
                .to_string(),
            ),
            2 => (
                lang.choose("не запущено", "not started").to_string(),
                lang.choose(
                    "✗ Самопиление не запустилось: рабочее дерево не чистое — закоммить или спрячь правки.",
                    "✗ Self-dev did not start: the working tree is dirty — commit or stash first.",
                )
                .to_string(),
            ),
            _ => (
                lang.choose("сбой", "failed").to_string(),
                format!(
                    "{} {code}.",
                    lang.choose(
                        "✗ Самопиление: сбой супервайзера, код",
                        "✗ Self-dev: supervisor failure, exit code"
                    )
                ),
            ),
        };
    }

    if code == 0 {
        (
            lang.choose("готово", "completed").to_string(),
            lang.choose("⏺ Готово.", "⏺ Done.").to_string(),
        )
    } else {
        (
            format!("{}:{code}", lang.choose("ошибка", "failed")),
            format!(
                "{} {code}.",
                lang.choose("✗ Завершилось с ошибкой, код", "✗ Failed with exit code")
            ),
        )
    }
}

impl App {
    /// Возвращает неотправленный текст (текущий запрос + ОЧЕРЕДЬ pending) в пустой инпут.
    /// Общее для отмены и «не залогинен»: набранное — включая поставленное в очередь —
    /// не должно молча теряться.
    fn restore_unsent_to_input(&mut self) {
        let mut restore: Vec<String> = self.restore_on_cancel.take().into_iter().collect();
        restore.extend(self.pending_messages.drain(..));
        if !restore.is_empty() && self.input.trim().is_empty() {
            self.input = restore.join("\n");
            self.cursor = self.input.len();
            self.history_index = None;
        }
    }

    pub(crate) fn drain_worker_events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            // Ран кончился — агент мог закоммитить или переключить ветку. Читаем ref ДО
            // разбора события: старый ран уже завершён, а следующий из очереди (его поднимет
            // `process_pending_messages`) ещё не стартовал. `AuthMissing` не в счёт: агент
            // не запускался.
            if matches!(
                event,
                WorkerEvent::Done(_)
                    | WorkerEvent::ChatDone(..)
                    | WorkerEvent::PlanReady(..)
                    | WorkerEvent::Cancelled
                    | WorkerEvent::Failed(_)
            ) {
                self.refresh_git_ref();
                // Тандемные гейты живут только пока воркер ждёт решения/ответа; любой
                // терминальный исход (в т.ч. Ctrl+C во время гейта → Cancelled) их гасит.
                self.tandem_gate = false;
                self.tandem_gate_tx = None;
                self.tandem_input_gate = false;
                self.tandem_input_tx = None;
                self.is_tandem_run = false;
            }

            match event {
                WorkerEvent::Line(line) => {
                    self.record_worker_activity(&line);
                    if let Some(path) = line.strip_prefix("Final brief: ") {
                        let path = path.to_string();
                        self.last_run = Some(path.clone());
                        self.push_system(line);
                        self.push_final_brief(&path);
                    } else {
                        self.push_system(line);
                    }
                }
                // Строки ответа копим — покажем «печатной машинкой» на ChatDone.
                WorkerEvent::ChatLine(line) => self.reveal_buffer.push(line),
                // Токен-стрим: дописываем в живой ответ (показывается сразу).
                WorkerEvent::StreamDelta(delta) => self.live_answer.push_str(&delta),
                // Рассуждение до ответа — копим отдельно, показываем в лоадере.
                WorkerEvent::ReasoningDelta(delta) => self.live_reasoning.push_str(&delta),
                WorkerEvent::Activity(line) => self.push_run_activity(line),
                WorkerEvent::Done(code) => {
                    self.running = false;
                    self.last_run_duration = self.run_started_at.map(|s| s.elapsed());
                    self.run_started_at = None;
                    self.run_label.clear();
                    self.run_token_estimate = None;
                    self.cancel_tx = None;
                    let (status, message) = run_finish_lines(self.dev_run, code, self.lang);
                    self.dev_run = false;
                    self.status = status;
                    self.flush_reveal_buffer();
                    self.push_system(message);
                    self.process_pending_messages();
                }
                WorkerEvent::ChatDone(provider, code, usage) => {
                    if let Some(usage) = usage {
                        self.usage.record(provider, usage);
                    }
                    self.running = false;
                    self.last_run_duration = self.run_started_at.map(|s| s.elapsed());
                    self.run_started_at = None;
                    self.run_label.clear();
                    self.run_token_estimate = None;
                    self.cancel_tx = None;
                    self.status = if code == 0 {
                        self.lang.choose("готово", "completed").to_string()
                    } else {
                        format!("{}:{code}", self.lang.choose("ошибка", "failed"))
                    };
                    // Ран завершился: фиксируем реплику пользователя в ленте (теперь
                    // уедет в нативный скроллбэк), до строк ответа/ошибки.
                    if let Some(turn) = self.live_turn.take() {
                        self.push_system(turn);
                    }
                    if code != 0 {
                        self.push_system(format!(
                            "{} {} {}.",
                            provider_display(provider.as_str(), self.lang),
                            self.lang
                                .choose("завершился с кодом", "finished with exit code"),
                            code
                        ));
                    }
                    // Ответ получен — возвращать в инпут нечего.
                    self.restore_on_cancel = None;
                    // Был ли токен-стрим (claude): тогда текст уже показан вживую.
                    let streamed = !self.live_answer.is_empty();
                    self.live_answer.clear();
                    self.live_reasoning.clear();
                    // Выделяем из ответа запрос выбора (clave-ask): прозу — в ленту, блок
                    // — в селектор. Парсим сырой буфер (find_ask_block срезает строку
                    // маркера целиком, поэтому префикс «⏺» не мешает).
                    let full = std::mem::take(&mut self.reveal_buffer).join("\n");
                    let (prose, ask) = parse_clave_ask(&full);
                    self.ask_prompt_pending = ask;
                    if streamed || prose.trim().is_empty() {
                        // Стримили вживую (или печатать нечего) → фиксируем без «печати».
                        self.commit_answer_text(&prose);
                    } else {
                        // codex / без стрима → плавная «печатная машинка».
                        self.reveal = Some(Reveal {
                            text: prose,
                            shown: 0,
                            started: Instant::now(),
                        });
                    }
                }
                WorkerEvent::PlanReady(provider, plan, code, usage) => {
                    if let Some(usage) = usage {
                        self.usage.record(provider, usage);
                    }
                    self.running = false;
                    self.last_run_duration = self.run_started_at.map(|s| s.elapsed());
                    self.run_started_at = None;
                    self.run_label.clear();
                    self.run_token_estimate = None;
                    self.cancel_tx = None;
                    self.flush_reveal_buffer();

                    let task = match std::mem::replace(&mut self.plan_flow, PlanFlow::None) {
                        PlanFlow::Planning { task } => Some(task),
                        _ => None,
                    };

                    if code == 0 && !plan.trim().is_empty() {
                        if let Some(task) = task {
                            self.pending_plan = Some(PendingPlan { task, plan });
                            self.status = self.lang.choose("план готов", "plan ready").to_string();
                        }
                    } else {
                        self.pending_plan = None;
                        self.status = self.lang.choose("ошибка плана", "plan failed").to_string();
                    }
                    // Провал плана не открывает гейт → очередь pending должна возобновиться,
                    // как после обычного завершения рана. На успехе pending_plan занят, и
                    // process_pending_messages сам выйдет по plan_gate_active.
                    self.process_pending_messages();
                }
                WorkerEvent::Cancelled => {
                    self.running = false;
                    self.last_run_duration = None;
                    self.run_started_at = None;
                    self.run_label.clear();
                    self.run_token_estimate = None;
                    self.cancel_tx = None;
                    self.reveal_buffer.clear();
                    self.reveal = None;
                    self.live_answer.clear();
                    self.live_reasoning.clear();
                    self.reset_ask();
                    self.status = self.lang.choose("остановлено", "stopped").to_string();
                    // Чат с «отложенной» репликой отменяем начисто: убираем её из живого
                    // блока (в ленту/скроллбэк она не попала) и возвращаем текст в инпут —
                    // без следа в диалоге. Для плана/движка (реплика уже в ленте) оставляем
                    // пометку об остановке.
                    let undone_chat = self.live_turn.take().is_some();
                    if !undone_chat {
                        self.push_system(
                            self.lang
                                .choose("⏹ Выполнение остановлено.", "⏹ Run stopped."),
                        );
                    }
                    // Возвращаем неотправленный текст (текущий запрос + очередь) в инпут,
                    // чтобы случайную отмену можно было поправить и отправить заново.
                    self.restore_unsent_to_input();
                }
                WorkerEvent::Failed(message) => {
                    self.running = false;
                    self.run_started_at = None;
                    self.run_label.clear();
                    self.run_token_estimate = None;
                    self.cancel_tx = None;
                    self.restore_on_cancel = None;
                    self.live_answer.clear();
                    self.live_reasoning.clear();
                    // Реплику фиксируем в ленте — ран дошёл до ошибки, это след попытки.
                    if let Some(turn) = self.live_turn.take() {
                        self.push_system(turn);
                    }
                    self.flush_reveal_buffer();
                    self.status = self.lang.choose("ошибка", "failed").to_string();
                    self.push_system(message);
                    self.process_pending_messages();
                }
                WorkerEvent::AuthMissing(provider) => {
                    self.running = false;
                    self.run_started_at = None;
                    self.run_label.clear();
                    self.run_token_estimate = None;
                    self.run_activity.clear();
                    self.cancel_tx = None;
                    self.reveal_buffer.clear();
                    self.reveal = None;
                    self.live_answer.clear();
                    self.live_reasoning.clear();
                    self.reset_ask();
                    // Не залогинены — реплику не отправили: убираем из живого блока и
                    // возвращаем неотправленный текст (текущий запрос + ОЧЕРЕДЬ) в инпут,
                    // чтобы после логина ничего не пришлось набирать заново. Раньше здесь
                    // pending_messages молча очищались — набранное в очередь терялось.
                    self.live_turn = None;
                    self.restore_unsent_to_input();
                    self.prompt_provider_login(provider);
                }
                WorkerEvent::PluginsLoaded(plugins) => self.plugins_loaded(plugins),
                WorkerEvent::PluginActionDone => self.plugin_action_done(),
                WorkerEvent::MarketplacesLoaded(markets) => self.marketplaces_loaded(markets),
                WorkerEvent::MarketplaceActionDone => self.marketplace_action_done(),
                WorkerEvent::TandemNeedsApproval => {
                    // Дебаты кончились без консенсуса — воркер заблокирован и ждёт. Проявляем
                    // накопленные шаги (иначе гейт всплыл бы без контекста дебатов) и показываем
                    // промпт прямо в ленте; ответ уйдёт в воркер по каналу из handle_input_key.
                    self.tandem_gate = true;
                    // Гасим ЖИВОЙ стрим последнего шага: иначе его сырой текст (с маркером)
                    // висит отдельным блоком-дублем, а лоадер пишет «Пишу ответ», хотя ждём тебя.
                    self.live_answer.clear();
                    self.live_reasoning.clear();
                    self.flush_reveal_buffer();
                    self.push_system(self.lang.choose(
                        "⚠ Консенсус не достигнут за раунды. Enter — исполнить последнюю версию · Esc — отмена, файлы не тронуты.",
                        "⚠ No consensus within the rounds. Enter — execute the latest version · Esc — cancel, files untouched.",
                    ));
                    self.status = self
                        .lang
                        .choose("нет консенсуса — Enter/Esc", "no consensus — Enter/Esc")
                        .to_string();
                }
                WorkerEvent::TandemStepEnd => {
                    // Шаг завершён: сразу выводим его в ленту (готовым блоком, как и было —
                    // через push_system) и гасим живой стрим, чтобы `⏺` не двоил тот же текст.
                    self.flush_reveal_buffer();
                    self.live_answer.clear();
                    self.live_reasoning.clear();
                }
                WorkerEvent::TandemNeedsInput(prompt) => {
                    // Исполнителю не хватает данных — вопросы уже в шаге. Гасим дубль-стрим
                    // (см. TandemNeedsApproval) и проявляем накопленное.
                    self.live_answer.clear();
                    self.live_reasoning.clear();
                    self.flush_reveal_buffer();
                    match prompt {
                        // Закрытый вопрос → тот же селектор, что в чате, но выбор питает тандем.
                        Some(prompt) => {
                            self.open_tandem_choice(prompt);
                            self.status = self.lang.choose("выбор", "choose").to_string();
                        }
                        // Открытый вопрос → текстовый ввод-гейт.
                        None => {
                            self.tandem_input_gate = true;
                            self.status = self
                                .lang
                                .choose(
                                    "нужны уточнения — ответь и Enter",
                                    "needs input — answer + Enter",
                                )
                                .to_string();
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;

    /// Каталог уникален на процесс И на вызов: параллельные прогоны иначе затирают
    /// файлы друг друга, и мутационный гейт получает случайные падения.
    fn temp_events_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = std::env::temp_dir().join(format!(
            "clave-events-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    /// App на своих временных путях. Через `App::new()` нельзя: она читает настоящий
    /// конфиг пользователя и при непройденном онбординге поднимает auth-probe процессы.
    pub(crate) fn app_for_events() -> (App, PathBuf) {
        let dir = temp_events_dir();
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
        app.lang = Language::Ru;
        app.onboarding = None;
        app.overlay = Overlay::None;
        // Живой git в юнит-тесте не нужен: события завершения зовут refresh_git_ref.
        app.git_ref_detector = |_| None;
        app.transcript.clear();
        (app, dir)
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

    /// И отмена, и «не залогинен» обязаны вернуть в инпут не только текущий запрос, но и
    /// ВСЮ очередь pending. Раньше AuthMissing очищал очередь молча — введённое терялось.
    #[test]
    fn restore_unsent_returns_current_message_and_the_queue() {
        let (mut app, _dir) = app_for_events();
        app.restore_on_cancel = Some("первое".to_string());
        app.pending_messages.push_back("второе".to_string());
        app.pending_messages.push_back("третье".to_string());

        app.restore_unsent_to_input();

        assert_eq!(
            app.input, "первое\nвторое\nтретье",
            "и текущий запрос, и вся очередь вернулись в инпут"
        );
        assert!(
            app.pending_messages.is_empty(),
            "очередь опустошена переносом в инпут, а не потеряна"
        );
    }

    /// Успешный чат: статус «готово», сырого кода в ленте нет, ответ уходит в «печать».
    #[test]
    fn chat_done_types_the_answer_out_without_shouting_the_exit_code() {
        let (mut app, _dir) = app_for_events();
        app.running = true;
        app.live_turn = Some("◆ вопрос".to_string());
        app.reveal_buffer = vec!["ответ модели".to_string()];

        app.tx
            .send(WorkerEvent::ChatDone(Provider::Codex, 0, None))
            .expect("send");
        app.drain_worker_events();

        assert_eq!(app.status, "готово");
        assert!(!app.running);
        assert!(
            app.transcript.iter().any(|line| line == "◆ вопрос"),
            "реплика пользователя фиксируется: {:?}",
            app.transcript
        );
        assert!(
            !app.transcript.iter().any(|line| line.contains("кодом")),
            "код 0 — не ошибка, в ленте его быть не должно: {:?}",
            app.transcript
        );
        let reveal = app
            .reveal
            .as_ref()
            .expect("без стрима — «печатная машинка»");
        assert_eq!(reveal.text, "ответ модели");
        assert!(
            !app.transcript.iter().any(|line| line == "ответ модели"),
            "текст ещё печатается, в ленте его пока нет"
        );
    }

    /// Ошибочный код — наоборот: и в статусе, и в ленте.
    #[test]
    fn chat_done_with_failure_code_reports_it() {
        let (mut app, _dir) = app_for_events();
        app.running = true;

        app.tx
            .send(WorkerEvent::ChatDone(Provider::Codex, 3, None))
            .expect("send");
        app.drain_worker_events();

        assert_eq!(app.status, "ошибка:3");
        assert!(
            app.transcript
                .iter()
                .any(|line| line.contains("завершился с кодом") && line.contains('3')),
            "сбой показываем: {:?}",
            app.transcript
        );
    }

    /// Был токен-стрим — текст уже показан вживую, «печатать» его второй раз нельзя:
    /// он фиксируется в ленте сразу.
    #[test]
    fn streamed_answer_is_committed_at_once_without_a_second_reveal() {
        let (mut app, _dir) = app_for_events();
        app.running = true;
        app.live_answer = "ответ модели".to_string();
        app.reveal_buffer = vec!["ответ модели".to_string()];

        app.tx
            .send(WorkerEvent::ChatDone(Provider::Claude, 0, None))
            .expect("send");
        app.drain_worker_events();

        assert!(
            app.reveal.is_none(),
            "стримленный ответ не «печатается» снова"
        );
        assert_eq!(app.transcript, vec!["ответ модели"]);
        assert!(app.live_answer.is_empty());
    }

    /// План готов: задача из PlanFlow обязана доехать до гейта вместе с планом.
    #[test]
    fn plan_ready_keeps_the_task_and_opens_the_gate() {
        let (mut app, _dir) = app_for_events();
        app.running = true;
        app.plan_flow = PlanFlow::Planning {
            task: "почини футер".to_string(),
        };

        app.tx
            .send(WorkerEvent::PlanReady(
                Provider::Codex,
                "1. шаг".to_string(),
                0,
                None,
            ))
            .expect("send");
        app.drain_worker_events();

        let plan = app.pending_plan.as_ref().expect("план ждёт подтверждения");
        assert_eq!(plan.task, "почини футер");
        assert_eq!(plan.plan, "1. шаг");
        assert_eq!(app.status, "план готов");
    }

    /// Пустой план — не план: гейт не открывается даже при нулевом коде.
    #[test]
    fn empty_plan_is_a_failure_not_a_gate() {
        let (mut app, _dir) = app_for_events();
        app.running = true;
        app.plan_flow = PlanFlow::Planning {
            task: "почини футер".to_string(),
        };

        app.tx
            .send(WorkerEvent::PlanReady(
                Provider::Codex,
                "   \n".to_string(),
                0,
                None,
            ))
            .expect("send");
        app.drain_worker_events();

        assert!(app.pending_plan.is_none(), "подтверждать нечего");
        assert_eq!(app.status, "ошибка плана");
    }

    /// Отмена чата: реплика уходит без следа, пометки об остановке в ленте нет.
    #[test]
    fn cancelled_chat_leaves_no_trace_in_the_transcript() {
        let (mut app, _dir) = app_for_events();
        app.running = true;
        app.live_turn = Some("◆ вопрос".to_string());

        app.tx.send(WorkerEvent::Cancelled).expect("send");
        app.drain_worker_events();

        assert_eq!(app.status, "остановлено");
        assert!(
            !app.transcript
                .iter()
                .any(|line| line.contains("Выполнение остановлено")),
            "отменённый чат не оставляет пометки: {:?}",
            app.transcript
        );
    }

    /// Гейт тандема: событие поднимает флаг и проявляет накопленные дебаты с промптом;
    /// любой терминальный исход (здесь Cancelled после отказа) гейт гасит и снимает канал.
    #[test]
    fn tandem_gate_event_opens_and_terminal_event_closes_it() {
        let (mut app, _dir) = app_for_events();
        app.running = true;
        app.reveal_buffer = vec!["🅐 предложение".to_string(), "🅒 возражение".to_string()];

        app.tx.send(WorkerEvent::TandemNeedsApproval).expect("send");
        app.drain_worker_events();

        assert!(app.tandem_gate, "событие открывает гейт");
        assert!(
            app.reveal_buffer.is_empty(),
            "накопленные дебаты проявлены перед решением"
        );
        assert!(
            app.transcript
                .iter()
                .any(|l| l.contains("Консенсус не достигнут")),
            "промпт гейта виден в ленте: {:?}",
            app.transcript
        );

        // Во время гейта канал ответа занят (в проде его ставит start_tandem).
        let (dummy_tx, _rx) = std::sync::mpsc::channel::<TandemGate>();
        app.tandem_gate_tx = Some(dummy_tx);

        app.tx.send(WorkerEvent::Cancelled).expect("send");
        app.drain_worker_events();
        assert!(!app.tandem_gate, "терминальный исход закрывает гейт");
        assert!(app.tandem_gate_tx.is_none(), "канал ответа снят");
    }

    /// Ввод-гейт «нужны уточнения»: событие поднимает флаг и проявляет вопросы; терминальный
    /// исход его гасит и снимает канал ответа.
    #[test]
    fn tandem_input_gate_event_opens_and_terminal_event_closes_it() {
        let (mut app, _dir) = app_for_events();
        app.running = true;
        app.reveal_buffer = vec!["🅐 вопросы к тебе".to_string()];

        app.tx
            .send(WorkerEvent::TandemNeedsInput(None))
            .expect("send");
        app.drain_worker_events();
        assert!(app.tandem_input_gate, "событие открывает ввод-гейт");
        assert!(app.reveal_buffer.is_empty(), "вопросы проявлены");

        let (dummy_tx, _rx) = std::sync::mpsc::channel::<String>();
        app.tandem_input_tx = Some(dummy_tx);
        app.tx
            .send(WorkerEvent::ChatDone(Provider::Claude, 0, None))
            .expect("send");
        app.drain_worker_events();
        assert!(
            !app.tandem_input_gate,
            "терминальный исход закрывает ввод-гейт"
        );
        assert!(app.tandem_input_tx.is_none(), "канал ответа снят");
    }

    /// Шаг тандема фиксируется в ленте СРАЗУ (по TandemStepEnd), а живой стрим гасится —
    /// чтобы `⏺` не двоил готовый блок и шаги не копились до гейта/конца.
    #[test]
    fn tandem_step_end_commits_step_and_clears_live_stream() {
        let (mut app, _dir) = app_for_events();
        app.running = true;
        app.reveal_buffer = vec!["🅐 Claude · раунд 1".to_string(), "тело шага".to_string()];
        app.live_answer = "сырой стрим".to_string();
        app.live_reasoning = "раздумья".to_string();

        app.tx.send(WorkerEvent::TandemStepEnd).expect("send");
        app.drain_worker_events();

        assert!(app.reveal_buffer.is_empty(), "буфер шага слит в ленту");
        assert!(
            app.transcript
                .iter()
                .any(|l| l.contains("🅐 Claude · раунд 1")),
            "блок шага в ленте: {:?}",
            app.transcript
        );
        assert!(
            app.live_answer.is_empty() && app.live_reasoning.is_empty(),
            "живой стрим погашен"
        );
    }

    /// Отмена плана/движка (реплика уже в ленте): пометка нужна, а неотправленный
    /// текст возвращается в пустой инпут — вместе с очередью.
    #[test]
    fn cancelled_run_marks_the_stop_and_restores_the_unsent_text() {
        let (mut app, _dir) = app_for_events();
        app.running = true;
        app.live_turn = None;
        app.restore_on_cancel = Some("первый запрос".to_string());
        app.pending_messages.push_back("второй запрос".to_string());

        app.tx.send(WorkerEvent::Cancelled).expect("send");
        app.drain_worker_events();

        assert!(
            app.transcript
                .iter()
                .any(|line| line.contains("Выполнение остановлено")),
            "остановку показываем: {:?}",
            app.transcript
        );
        assert_eq!(app.input, "первый запрос\nвторой запрос");
        assert_eq!(app.cursor, app.input.len());
        assert!(app.pending_messages.is_empty());
    }

    /// Уже набранный черновик отмена затирать не смеет.
    #[test]
    fn cancel_never_overwrites_a_draft_already_typed() {
        let (mut app, _dir) = app_for_events();
        app.running = true;
        app.input = "мой черновик".to_string();
        app.cursor = app.input.len();
        app.restore_on_cancel = Some("старый запрос".to_string());

        app.tx.send(WorkerEvent::Cancelled).expect("send");
        app.drain_worker_events();

        assert_eq!(
            app.input, "мой черновик",
            "черновик пользователя неприкосновенен"
        );
    }

    #[test]
    fn dev_exit_code_is_an_outcome_not_an_error() {
        // /dev: 1 = «не сошлось» (агент не внёс правок). Это ИСХОД. Раньше футер писал
        // «ошибка:1», а в ленту падало «Clave завершился с кодом 1» — читалось как падение.
        let (status, message) = run_finish_lines(true, 1, Language::Ru);
        assert!(
            !status.contains("ошибка"),
            "не сошлось — это не ошибка: {status}"
        );
        assert!(
            !message.contains("кодом"),
            "в ленте не должно быть сырого кода: {message}"
        );
        assert!(message.contains("не сошлось") || message.contains("не довели"));

        let (status, _) = run_finish_lines(true, 0, Language::Ru);
        assert_eq!(status, "готово");

        let (_, message) = run_finish_lines(true, 2, Language::Ru);
        assert!(
            message.contains("дерево"),
            "код 2 — грязное дерево: {message}"
        );
    }

    #[test]
    fn plain_run_finish_is_human_and_keeps_the_code_only_on_failure() {
        let (_, ok) = run_finish_lines(false, 0, Language::Ru);
        assert_eq!(ok, "⏺ Готово.");
        let (status, bad) = run_finish_lines(false, 7, Language::Ru);
        assert!(status.contains("ошибка"));
        assert!(
            bad.contains('7'),
            "код сбоя всё же показываем для диагностики: {bad}"
        );
    }
}
