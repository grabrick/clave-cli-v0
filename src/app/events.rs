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

/// Человеческий финал прогона вместо машинного «Clave завершился с кодом N»: сырой код
/// выхода в ленте читается как падение, даже когда всё прошло хорошо. Показываем его
/// только там, где сбой действительно есть — там он и нужен, для диагностики.
///
/// Возвращает (статус для футера, строку в ленту).
pub(crate) fn run_finish_lines(code: i32, lang: Language) -> (String, String) {
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
                    let (status, message) = run_finish_lines(code, self.lang);
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
    fn run_finish_is_human_and_keeps_the_code_only_on_failure() {
        // «Clave завершился с кодом 0» читалось как падение, хотя всё прошло хорошо.
        let (_, ok) = run_finish_lines(0, Language::Ru);
        assert_eq!(ok, "⏺ Готово.");
        // А при реальном сбое код по-прежнему нужен — для диагностики.
        let (status, bad) = run_finish_lines(7, Language::Ru);
        assert!(status.contains("ошибка"));
        assert!(bad.contains('7'), "код сбоя показываем: {bad}");
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
