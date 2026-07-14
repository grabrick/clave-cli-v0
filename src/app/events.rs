use super::*;

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

/// Плавная «печатная машинка» для ответа: целиком готовый текст вскрывается
/// по символам со временем, пока полностью не уйдёт в историю.
pub(crate) struct Reveal {
    text: String,
    shown: usize,
    started: Instant,
}

/// Скорость «печати» (символов/сек). Короткие ответы появляются почти сразу,
/// длинные — заметно набираются. Прерывается любой клавишей (finish_reveal_now).
const REVEAL_CHARS_PER_SEC: usize = 600;

/// Сколько символов ответа должно быть «вскрыто» к моменту `elapsed_ms` при
/// текущей скорости, но не больше длины всего текста (`total`). Чистая функция —
/// чтобы раскадровку «печати» можно было проверить без таймеров и терминала.
fn reveal_chars_for(elapsed_ms: u128, total: usize) -> usize {
    let target = elapsed_ms.saturating_mul(REVEAL_CHARS_PER_SEC as u128) / 1000;
    (target as usize).min(total)
}

impl Reveal {
    /// Уже вскрытая часть текста (для отрисовки в живом блоке).
    pub(crate) fn shown_text(&self) -> String {
        self.text.chars().take(self.shown).collect()
    }
}

impl App {
    /// Идёт ли сейчас анимация (loader / reveal / footer-notice / shimmer / палитра).
    pub(crate) fn is_animating(&self) -> bool {
        // footer_right_changed_at намеренно НЕ учитываем (ротация раз в 8с не должна
        // будить простой). Палитру и reveal учитываем — у них живая анимация.
        self.running
            || self.reveal.is_some()
            || self.footer_notice.is_some()
            || self.overlay == Overlay::Effort
            || normalized_command_query(&self.input).is_some()
    }

    /// Двигает «печать» ответа по времени; по завершении фиксирует его в истории.
    pub(crate) fn advance_reveal(&mut self) {
        let finished = match &mut self.reveal {
            Some(reveal) => {
                let total = reveal.text.chars().count();
                reveal.shown = reveal_chars_for(reveal.started.elapsed().as_millis(), total);
                reveal.shown >= total
            }
            None => false,
        };
        if finished {
            self.commit_reveal();
        }
    }

    /// Мгновенно дописать ответ и зафиксировать (по любому нажатию клавиши).
    pub(crate) fn finish_reveal_now(&mut self) {
        if self.reveal.is_some() {
            self.commit_reveal();
        }
    }

    /// Переносит готовый reveal в историю (скроллбэк) и запускает отложенную очередь.
    fn commit_reveal(&mut self) {
        let text = self
            .reveal
            .take()
            .map(|reveal| reveal.text)
            .unwrap_or_default();
        self.commit_answer_text(&text);
    }

    /// Фиксирует готовый текст ответа в ленте и продолжает: открывает отложенный
    /// селектор (clave-ask), иначе берёт следующее сообщение из очереди.
    fn commit_answer_text(&mut self, prose: &str) {
        if !prose.is_empty() {
            for line in prose.split('\n') {
                self.push_system(line.to_string());
            }
        }
        if self.ask_prompt_pending.is_some() {
            self.open_pending_ask();
        } else {
            self.process_pending_messages();
        }
    }

    /// Накопленный буфер ответа — сразу в историю (для не-чатовых путей: план, отмена).
    fn flush_reveal_buffer(&mut self) {
        for line in std::mem::take(&mut self.reveal_buffer) {
            self.push_system(line);
        }
    }

    pub(crate) fn push_run_activity(&mut self, activity: impl Into<String>) {
        let activity = activity.into();
        if activity.trim().is_empty() {
            return;
        }

        self.run_activity.push_back(activity);
        while self.run_activity.len() > 5 {
            self.run_activity.pop_front();
        }
    }

    pub(crate) fn record_worker_activity(&mut self, line: &str) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return;
        }

        if let Some(path) = trimmed.strip_prefix("Final brief: ") {
            self.push_run_activity(format!(
                "{} {}",
                self.lang.choose("итог:", "final:"),
                truncate_chars(path, 96)
            ));
        } else {
            self.push_run_activity(truncate_chars(trimmed, 120));
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
                    let mut restore: Vec<String> =
                        self.restore_on_cancel.take().into_iter().collect();
                    restore.extend(self.pending_messages.drain(..));
                    if !restore.is_empty() && self.input.trim().is_empty() {
                        self.input = restore.join("\n");
                        self.cursor = self.input.len();
                        self.history_index = None;
                    }
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
                    self.pending_messages.clear();
                    // Не залогинены — реплику не отправили: убираем из живого блока и
                    // возвращаем текст в инпут, чтобы повторить после логина.
                    self.live_turn = None;
                    if let Some(text) = self.restore_on_cancel.take() {
                        if self.input.trim().is_empty() {
                            self.input = text;
                            self.cursor = self.input.len();
                            self.history_index = None;
                        }
                    }
                    self.prompt_provider_login(provider);
                }
            }
        }
    }

    pub(crate) fn push_final_brief(&mut self, path: &str) {
        match final_brief_lines_for_chat(path, self.lang) {
            Ok(lines) => {
                self.push_system(self.lang.choose("⏺ Итоговый ответ", "⏺ Final answer"));
                for line in lines {
                    self.push_system(line);
                }
            }
            Err(err) => self.push_system(format!(
                "{} {}",
                self.lang.choose(
                    "Не удалось прочитать итоговый ответ:",
                    "Failed to read final answer:"
                ),
                err
            )),
        }
    }
}

#[cfg(test)]
mod tests {
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
    fn app_for_events() -> (App, PathBuf) {
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

    fn reveal_of(text: &str) -> Reveal {
        Reveal {
            text: text.to_string(),
            shown: 0,
            started: Instant::now(),
        }
    }

    fn ask_prompt(question: &str) -> AskPrompt {
        AskPrompt {
            questions: vec![AskQuestion {
                question: question.to_string(),
                multi: false,
                options: vec![AskOption {
                    label: "Да".to_string(),
                    note: None,
                }],
                allow_custom: true,
            }],
        }
    }

    /// Главный контракт ответа: то, что прислал провайдер, обязано ОКАЗАТЬСЯ В ЛЕНТЕ —
    /// построчно. Пустышка вместо commit_reveal/commit_answer_text = ответ пришёл и молча
    /// исчез: пользователь ждал прогон, а на экране пусто.
    #[test]
    fn committed_answer_lands_in_the_transcript_line_by_line() {
        let (mut app, _dir) = app_for_events();
        app.reveal = Some(reveal_of("строка один\nстрока два"));

        app.finish_reveal_now();

        assert_eq!(app.transcript, vec!["строка один", "строка два"]);
        assert!(app.reveal.is_none(), "зафиксированный ответ снимается");
    }

    /// Пустой ответ не должен оставлять в ленте пустую строку (снятое `!` в
    /// commit_answer_text печатает ровно её).
    #[test]
    fn empty_answer_adds_no_lines() {
        let (mut app, _dir) = app_for_events();
        app.commit_reveal();
        assert!(app.transcript.is_empty(), "пусто — значит нечего печатать");
    }

    /// Отложенный селектор открывается ровно после того, как проза допечаталась.
    #[test]
    fn pending_ask_opens_right_after_the_prose_is_committed() {
        let (mut app, _dir) = app_for_events();
        app.ask_prompt_pending = Some(ask_prompt("Продолжаем?"));
        app.reveal = Some(reveal_of("вот варианты"));

        app.finish_reveal_now();

        assert_eq!(app.transcript, vec!["вот варианты"]);
        assert!(app.ask_active(), "селектор обязан открыться");
    }

    /// «Печать» действительно едет по времени и доводится до конца.
    #[test]
    fn advance_reveal_types_out_over_time_and_commits_at_the_end() {
        let (mut app, _dir) = app_for_events();
        let text: String = "я".repeat(400);
        app.reveal = Some(Reveal {
            text: text.clone(),
            shown: 0,
            started: Instant::now() - Duration::from_millis(200),
        });

        app.advance_reveal();

        let reveal = app.reveal.as_ref().expect("длинный ответ ещё печатается");
        let shown = reveal.shown_text().chars().count();
        assert!(shown > 0, "за 200мс часть текста вскрыта, а не ноль");
        assert!(shown < 400, "но не весь: это «печать», а не вспышка");
        assert!(
            app.transcript.is_empty(),
            "недопечатанное в ленту не уходит"
        );

        // Прошло достаточно — текст обязан доехать в ленту целиком.
        app.reveal = Some(Reveal {
            text: text.clone(),
            shown: 0,
            started: Instant::now() - Duration::from_secs(5),
        });
        app.advance_reveal();

        assert!(app.reveal.is_none(), "допечатанный ответ фиксируется");
        assert_eq!(app.transcript, vec![text]);
    }

    /// Буфер ответа (не-чатовые пути: план, ошибка, завершение) обязан вылиться в ленту.
    #[test]
    fn flush_reveal_buffer_pours_every_line_into_the_transcript() {
        let (mut app, _dir) = app_for_events();
        app.reveal_buffer = vec!["первая".to_string(), "вторая".to_string()];

        app.flush_reveal_buffer();

        assert_eq!(app.transcript, vec!["первая", "вторая"]);
        assert!(app.reveal_buffer.is_empty(), "буфер опустошён");
    }

    /// Лента активности держит ровно пять последних строк — не больше и не меньше.
    #[test]
    fn run_activity_keeps_the_last_five_lines_and_skips_blanks() {
        let (mut app, _dir) = app_for_events();
        for i in 0..7 {
            app.push_run_activity(format!("шаг {i}"));
        }

        let lines: Vec<String> = app.run_activity.iter().cloned().collect();
        assert_eq!(lines, vec!["шаг 2", "шаг 3", "шаг 4", "шаг 5", "шаг 6"]);

        app.push_run_activity("   ");
        assert_eq!(app.run_activity.len(), 5, "пустая строка не добавляется");
    }

    /// Строка воркера про итоговый файл показывается как «итог: …», обычная — как есть.
    #[test]
    fn worker_activity_marks_the_final_brief_and_ignores_blanks() {
        let (mut app, _dir) = app_for_events();
        app.record_worker_activity("Final brief: /tmp/run/brief.md");
        assert_eq!(
            app.run_activity.back().map(String::as_str),
            Some("итог: /tmp/run/brief.md")
        );

        app.record_worker_activity("  правлю модуль  ");
        assert_eq!(
            app.run_activity.back().map(String::as_str),
            Some("правлю модуль")
        );

        app.record_worker_activity("   ");
        assert_eq!(app.run_activity.len(), 2, "пустая строка не пишется");
    }

    /// Итоговая сводка обязана появиться в ленте — с заголовком и содержимым.
    #[test]
    fn final_brief_is_pushed_into_the_transcript() {
        let (mut app, dir) = app_for_events();
        let brief = dir.join("brief.md");
        fs::write(&brief, "## Current Spec\nсделать X\n").expect("write brief");

        app.push_final_brief(&brief.to_string_lossy());

        assert!(
            app.transcript.iter().any(|line| line == "⏺ Итоговый ответ"),
            "заголовок сводки: {:?}",
            app.transcript
        );
        assert!(app.transcript.iter().any(|line| line == "## Текущая спека"));
        assert!(app.transcript.iter().any(|line| line == "сделать X"));
    }

    #[test]
    fn unreadable_final_brief_is_reported_in_the_transcript() {
        let (mut app, dir) = app_for_events();

        app.push_final_brief(&dir.join("нет-такого.md").to_string_lossy());

        assert!(
            app.transcript
                .last()
                .is_some_and(|line| line.contains("Не удалось прочитать итоговый ответ")),
            "ошибку чтения показываем: {:?}",
            app.transcript
        );
    }

    /// Анимация будится ровно пятью причинами — и ни одной меньше: иначе экран замирает
    /// с недокрученным лоадером/reveal.
    #[test]
    fn animation_wakes_on_every_live_thing_and_sleeps_otherwise() {
        let (mut app, _dir) = app_for_events();
        app.running = false;
        app.reveal = None;
        app.footer_notice = None;
        app.overlay = Overlay::None;
        app.input.clear();
        assert!(!app.is_animating(), "простой — анимации нет");

        app.running = true;
        assert!(app.is_animating(), "идёт прогон");
        app.running = false;

        app.reveal = Some(reveal_of("текст"));
        assert!(app.is_animating(), "печатается ответ");
        app.reveal = None;

        app.footer_notice = Some(("готово".to_string(), Instant::now()));
        assert!(app.is_animating(), "всплывашка футера");
        app.footer_notice = None;

        app.overlay = Overlay::Effort;
        assert!(app.is_animating(), "палитра усилий");
        app.overlay = Overlay::None;

        app.input = "/he".to_string();
        assert!(app.is_animating(), "открыта палитра команд");
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
    fn reveal_unveils_gradually_then_caps_at_total() {
        let total = 300;
        // В нулевой момент ещё ничего не вскрыто.
        assert_eq!(reveal_chars_for(0, total), 0);
        // Со временем вскрывается строго больше — это и есть «печать», а не вспышка.
        let early = reveal_chars_for(100, total);
        let later = reveal_chars_for(250, total);
        assert!(
            early > 0 && early < total,
            "за 100мс — часть текста: {early}"
        );
        assert!(later > early, "позже вскрыто больше: {later} > {early}");
        // 600 симв/сек: 100мс ⇒ 60 символов, 250мс ⇒ 150.
        assert_eq!(early, 60);
        assert_eq!(later, 150);
        // Дольше длины текста расти нельзя — переполнения нет.
        assert_eq!(reveal_chars_for(10_000, total), total);
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

    #[test]
    fn reveal_shown_text_is_a_char_prefix() {
        let reveal = Reveal {
            text: "Привет, мир".to_string(),
            shown: 6,
            started: Instant::now(),
        };
        // Режем по символам, а не байтам — кириллица не должна ломаться.
        assert_eq!(reveal.shown_text(), "Привет");
    }
}
