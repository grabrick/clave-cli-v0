use crate::prelude::*;
use crate::*;

/// Ищет строку-СИГНАЛ (строго `TANDEM: CONSENSUS` / `TANDEM: CONTINUE`, снизу вверх):
/// CONSENSUS → true. Дефолт false (= CONTINUE) — безопаснее продолжить, чем ложно
/// согласиться (P1). Строгий разбор не даёт упоминанию `TANDEM:` в прозе завершить дебаты.
pub(crate) fn parse_tandem_signal(text: &str) -> bool {
    for line in text.lines().rev() {
        // Считаем строку сигналом, только если после снятия markdown-обрамления она
        // НАЧИНАЕТСЯ с `TANDEM:`, а остаток — ровно CONSENSUS или CONTINUE. Иначе фраза
        // из рассуждений вроде «...output TANDEM: CONSENSUS when done» дала бы ложный
        // консенсус и преждевременно завершила бы дебаты.
        let cleaned = line
            .trim()
            .trim_matches(|c: char| c == '*' || c == '`' || c == '>' || c == ' ');
        let upper = cleaned.to_uppercase();
        let Some(rest) = upper.strip_prefix("TANDEM:") else {
            continue;
        };
        return rest.trim() == "CONSENSUS";
    }
    false
}

/// Исполнитель запросил ввод: последняя значимая строка — сигнал `TANDEM: NEED_INPUT`.
/// Тогда задача/данные неясны, и продолжать дебаты бессмысленно — надо спросить пользователя.
pub(crate) fn tandem_needs_input(text: &str) -> bool {
    for line in text.lines().rev() {
        let cleaned = line.trim().trim_matches(['*', '`', '>', ' ']);
        if cleaned.is_empty() {
            continue;
        }
        let upper = cleaned.to_uppercase();
        let Some(rest) = upper.strip_prefix("TANDEM:") else {
            return false; // значимая строка, но не сигнал → не запрос ввода
        };
        return rest.trim() == "NEED_INPUT";
    }
    false
}

/// Если строка — протокольный сигнал `TANDEM: <СЛОВО>`, возвращает её человеческий ХВОСТ
/// после маркера (для CONTINUE — сводку возражений, для CONSENSUS — оговорку), иначе None.
/// Срезаем ЛЮБОЙ маркер, а не три известных: модель порой сочиняет свой (`TANDEM: CLOSED`),
/// и он не должен протечь в ленту. Детекция самого СИГНАЛА (parse_tandem_signal /
/// tandem_needs_input) остаётся строгой — доверяем только каноничным словам.
fn tandem_marker_tail(line: &str) -> Option<String> {
    let cleaned = line.trim().trim_matches(['*', '`', '>', ' ']);
    // «TANDEM:» — ровно 7 ASCII-байт в любом регистре; `get` не паникует на границе символа.
    if !cleaned
        .get(..7)
        .is_some_and(|p| p.eq_ignore_ascii_case("tandem:"))
    {
        return None;
    }
    let after_colon = cleaned[7..].trim_start();
    // Первое «слово» (буквы/подчёркивание) после двоеточия — сам маркер; срезаем его.
    let word_len = after_colon
        .find(|c: char| !c.is_ascii_alphabetic() && c != '_')
        .unwrap_or(after_colon.len());
    if word_len == 0 {
        return None; // после «TANDEM:» сразу не-слово — это не сигнал
    }
    let tail = after_colon[word_len..]
        .trim()
        .trim_start_matches(['—', '-', ':', ' ']);
    Some(tail.to_string())
}

/// Текст шага для показа/ленты: срезает протокольные строки-сигналы, сохраняя человеческий
/// хвост (сводку возражений после CONTINUE). Пустые строки-маркеры уходят целиком.
fn strip_tandem_markers(text: &str) -> String {
    let mut kept = Vec::new();
    for line in text.lines() {
        match tandem_marker_tail(line) {
            Some(tail) if tail.is_empty() => {} // чистый маркер → выкидываем строку
            Some(tail) => kept.push(tail),
            None => kept.push(line.to_string()),
        }
    }
    kept.join("\n").trim().to_string()
}

pub(crate) struct TandemStep {
    pub(crate) text: String,
    pub(crate) code: i32,
    pub(crate) usage: Option<RunUsage>,
}

pub(crate) enum TandemResult {
    Completed(i32, Option<RunUsage>),
    Cancelled,
}

/// Решение пользователя на гейте «нет консенсуса»: исполнять последнюю версию или
/// отменить, не тронув файлы. Приходит по каналу в заблокированный воркер.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TandemGate {
    Execute,
    Abort,
}

/// Лента тандема, передаётся целиком в каждый промпт (P6: усечение при росте).
struct TandemTranscript {
    entries: Vec<String>,
}

impl TandemTranscript {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn push(&mut self, who: &str, phase: &str, text: &str) {
        self.entries
            .push(format!("[{who} · {phase}]\n{}", text.trim()));
    }

    fn render(&self) -> String {
        let full = self.entries.join("\n\n");
        if full.len() <= 12_000 || self.entries.len() <= 4 {
            return full;
        }
        // P6: оставляем первую запись + хвост (последние 3)
        let head = &self.entries[0];
        let tail = &self.entries[self.entries.len() - 3..];
        format!(
            "{head}\n\n…[ранние раунды усечены]…\n\n{}",
            tail.join("\n\n")
        )
    }
}

fn tandem_accumulate(total: &mut RunUsage, usage: &Option<RunUsage>) {
    if let Some(u) = usage {
        total.input += u.input;
        total.output += u.output;
        total.cache_read += u.cache_read;
        total.cache_creation += u.cache_creation;
        total.cost_usd += u.cost_usd;
    }
}

fn emit_tandem_step(tx: &Sender<WorkerEvent>, marker: &str, who: &str, phase: &str, text: &str) {
    // Пустая строка-разделитель ПЕРЕД шагом, а не после: иначе последний шаг
    // оставляет хвостовую пустую строку, и над inactive-лоадером получается двойной
    // отступ (хвост шага + gap_top).
    let _ = tx.send(WorkerEvent::ChatLine(String::new()));
    let _ = tx.send(WorkerEvent::ChatLine(format!("{marker} {who} · {phase}")));
    for line in text.trim().lines() {
        let _ = tx.send(WorkerEvent::ChatLine(line.to_string()));
    }
    // Шаг выведен полностью — фиксируем его в ленте сразу (не копим до гейта/конца).
    let _ = tx.send(WorkerEvent::TandemStepEnd);
}

fn tandem_notice(tx: &Sender<WorkerEvent>, text: String) {
    let _ = tx.send(WorkerEvent::Line(text));
}

/// Человеческая статус-строка в ленту (вместо сырого `TANDEM: …`): «✓ Консенсус» и т.п.
/// Идёт отдельной строкой после шага, с отступом — как продолжение блока шага.
fn emit_tandem_status(tx: &Sender<WorkerEvent>, text: &str) {
    let _ = tx.send(WorkerEvent::ChatLine(format!("  {text}")));
    let _ = tx.send(WorkerEvent::TandemStepEnd);
}

fn opt_usage(total: RunUsage) -> Option<RunUsage> {
    if total == RunUsage::default() {
        None
    } else {
        Some(total)
    }
}

/// Оркестратор тандема: дебаты до консенсуса → исполнение → ревью → правка →
/// подтверждение. Серия вызовов `run_provider_once`; стрим шагов в чат.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_tandem(
    executor: &'static str,
    critic: &'static str,
    executor_effort: &str,
    critic_effort: &str,
    task: &str,
    rounds: usize,
    work_dir: &Path,
    cancel_rx: Receiver<()>,
    gate_rx: Receiver<TandemGate>,
    input_rx: Receiver<String>,
    tx: Sender<WorkerEvent>,
    lang: Language,
) -> io::Result<TandemResult> {
    let run_step = |provider: &'static str, effort: &str, prompt: &str, access: RunAccess| {
        run_provider_once(
            provider, effort, prompt, work_dir, access, lang, &tx, &cancel_rx,
        )
    };
    run_tandem_with(
        run_step,
        executor,
        critic,
        executor_effort,
        critic_effort,
        task,
        rounds,
        &cancel_rx,
        &gate_rx,
        &input_rx,
        &tx,
        lang,
    )
}

/// Оркестрация тандема в отрыве от запуска процессов: `run_step` — один вызов провайдера
/// (в проде — `run_provider_once`), `None` = отмена. Шов существует ради тестов: иначе
/// последовательность фаз и обработка ошибок проверяемы только реальными CLI.
#[allow(clippy::too_many_arguments)]
fn run_tandem_with<R>(
    mut run_step: R,
    executor: &'static str,
    critic: &'static str,
    executor_effort: &str,
    critic_effort: &str,
    task: &str,
    rounds: usize,
    cancel_rx: &Receiver<()>,
    gate_rx: &Receiver<TandemGate>,
    input_rx: &Receiver<String>,
    tx: &Sender<WorkerEvent>,
    lang: Language,
) -> io::Result<TandemResult>
where
    R: FnMut(&'static str, &str, &str, RunAccess) -> io::Result<Option<TandemStep>>,
{
    let mut transcript = TandemTranscript::new();
    let mut total = RunUsage::default();
    let executor_name = provider_display(executor, lang);
    let critic_name = provider_display(critic, lang);
    let exec_role = lang.choose("Исполнитель", "Executor");
    let crit_role = lang.choose("Критик", "Critic");

    // P5: предупреждение о возможных изменённых файлах при прерывании после исполнения.
    let dirty_notice = |tx: &Sender<WorkerEvent>| {
        tandem_notice(
            tx,
            lang.choose(
                "⚠ Файлы были изменены до прерывания — проверь рабочую директорию.",
                "⚠ Files were modified before interruption — check the working directory.",
            )
            .to_string(),
        );
    };

    // ФАЗА ДЕБАТОВ
    let mut consensus = false;
    let mut rounds_done = 0usize;
    'debate: for round in 1..=rounds.max(1) {
        rounds_done = round;

        // Предложение исполнителя. Если задача/данные неясны — исполнитель шлёт NEED_INPUT,
        // мы спрашиваем пользователя, вливаем ответ в ленту и ПЕРЕ-предлагаем (без рестарта).
        let exec_code = loop {
            let propose = tandem_propose_prompt(task, &transcript.render(), lang);
            let step = match run_step(executor, executor_effort, &propose, RunAccess::PlanReadonly)?
            {
                Some(s) => s,
                None => return Ok(TandemResult::Cancelled),
            };
            tandem_accumulate(&mut total, &step.usage);
            // Показываем ДО кода возврата (при ошибке причина — в выводе) и БЕЗ протокольных
            // маркеров: сырой `TANDEM: …` — служебный сигнал, не для глаз пользователя.
            // Закрытый вопрос? Исполнитель прикладывает блок ```clave-ask — прозу показываем,
            // сам JSON-блок уводим в селектор (иначе мелькал бы в ленте). Обычное предложение
            // блока не содержит → prose = весь текст, ask_prompt = None.
            let (prose, ask_prompt) = parse_clave_ask(&step.text);
            let display = strip_tandem_markers(&prose);
            emit_tandem_step(
                tx,
                "🅐",
                executor_name,
                &format!("{} {round} · {}", lang.choose("раунд", "round"), exec_role),
                &display,
            );

            if step.code == 0 && tandem_needs_input(&step.text) {
                // Исполнитель просит уточнений: вопросы — в ленту как контекст, дальше ждём
                // ответ пользователя (по каналу) — текстом или выбором из селектора, если
                // приложен блок — затем пере-предлагаем с ним.
                transcript.push(exec_role, lang.choose("вопрос", "question"), &display);
                emit_tandem_status(
                    tx,
                    if ask_prompt.is_some() {
                        lang.choose(
                            "⚠ Нужен выбор — стрелки, Enter (или свой ответ)",
                            "⚠ Choose — arrows, Enter (or type your own)",
                        )
                    } else {
                        lang.choose(
                            "⚠ Нужны уточнения — ответь и Enter",
                            "⚠ Needs input — answer and press Enter",
                        )
                    },
                );
                let _ = tx.send(WorkerEvent::TandemNeedsInput(ask_prompt));
                let answer = loop {
                    if cancel_rx.try_recv().is_ok() {
                        return Ok(TandemResult::Cancelled);
                    }
                    match input_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                        Ok(a) => break Some(a),
                        // Нет UI (headless) — спрашивать некого; идём дальше без ответа.
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break None,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    }
                };
                match answer {
                    Some(a) => {
                        transcript.push(
                            lang.choose("Пользователь", "User"),
                            lang.choose("уточнение", "clarification"),
                            a.trim(),
                        );
                        continue; // пере-предлагаем — теперь с ответом в ленте
                    }
                    None => break step.code, // headless: вопросы уже в ленте, идём к критику
                }
            }

            transcript.push(
                exec_role,
                &format!(
                    "{} {round}",
                    lang.choose("предложение, раунд", "proposal, round")
                ),
                &display,
            );
            break step.code;
        };
        if exec_code != 0 {
            tandem_notice(
                tx,
                format!(
                    "{} {}",
                    executor_name,
                    lang.choose("вернул ошибку", "returned an error")
                ),
            );
            return Ok(TandemResult::Completed(exec_code, opt_usage(total)));
        }

        let challenge =
            tandem_challenge_prompt(task, &transcript.render(), round, rounds.max(1), lang);
        let step = match run_step(critic, critic_effort, &challenge, RunAccess::PlanReadonly)? {
            Some(s) => s,
            None => return Ok(TandemResult::Cancelled),
        };
        tandem_accumulate(&mut total, &step.usage);
        let display = strip_tandem_markers(&step.text);
        emit_tandem_step(
            tx,
            "🅒",
            critic_name,
            &format!("{} {round} · {}", lang.choose("раунд", "round"), crit_role),
            &display,
        );
        transcript.push(
            crit_role,
            &format!(
                "{} {round}",
                lang.choose("критика, раунд", "critique, round")
            ),
            &display,
        );
        if step.code != 0 {
            tandem_notice(
                tx,
                format!(
                    "{} {}",
                    critic_name,
                    lang.choose("вернул ошибку", "returned an error")
                ),
            );
            return Ok(TandemResult::Completed(step.code, opt_usage(total)));
        }

        // Сырой маркер спрятан — вместо него человеческий статус раунда.
        if parse_tandem_signal(&step.text) {
            consensus = true;
            emit_tandem_status(
                tx,
                lang.choose("✓ Консенсус достигнут", "✓ Consensus reached"),
            );
            break 'debate;
        }
        emit_tandem_status(
            tx,
            lang.choose(
                "↳ Есть замечания — продолжаем",
                "↳ Objections raised — continuing",
            ),
        );
    }
    // Нет консенсуса → НЕ пишем молча. Раньше отсюда молча шли в исполнение с доступом на
    // запись — ровно это давало «внезапно изменённые файлы»: агенты не договорились, а файлы
    // уже переписаны. Теперь показываем гейт и ЖДЁМ решения пользователя, продолжая слушать
    // отмену. Пока ждём — файлы не тронуты (исполнение ещё впереди).
    if !consensus {
        let _ = tx.send(WorkerEvent::TandemNeedsApproval);
        loop {
            if cancel_rx.try_recv().is_ok() {
                return Ok(TandemResult::Cancelled);
            }
            match gate_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(TandemGate::Execute) => break,
                // Отказ пользователя или пропавший UI — исполнять нечего, файлы не тронуты.
                Ok(TandemGate::Abort) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Ok(TandemResult::Cancelled);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            }
        }
    }

    // ФАЗА ИСПОЛНЕНИЯ
    if cancel_rx.try_recv().is_ok() {
        return Ok(TandemResult::Cancelled);
    }
    let execute = tandem_execute_prompt(task, &transcript.render(), lang);
    let step = match run_step(executor, executor_effort, &execute, RunAccess::PlanExecute)? {
        Some(s) => s,
        None => {
            dirty_notice(tx);
            return Ok(TandemResult::Cancelled);
        }
    };
    tandem_accumulate(&mut total, &step.usage);
    // Вывод исполнения показываем ДО кода: при ошибке (как в прогоне пользователя — код 1) причина
    // была в самом выводе Claude, а он глотался — оставался только «вернул ошибку», нечего отлаживать.
    let display = strip_tandem_markers(&step.text);
    emit_tandem_step(
        tx,
        "🅐",
        executor_name,
        &format!("{} · {}", lang.choose("исполнение", "execution"), exec_role),
        &display,
    );
    transcript.push(exec_role, lang.choose("исполнение", "execution"), &display);
    if step.code != 0 {
        dirty_notice(tx);
        tandem_notice(
            tx,
            format!(
                "{} {}",
                executor_name,
                lang.choose("вернул ошибку", "returned an error")
            ),
        );
        return Ok(TandemResult::Completed(step.code, opt_usage(total)));
    }

    // ФАЗА РЕВЬЮ
    let review = tandem_review_prompt(task, &transcript.render(), lang);
    let step = match run_step(critic, critic_effort, &review, RunAccess::PlanReadonly)? {
        Some(s) => s,
        None => {
            dirty_notice(tx);
            return Ok(TandemResult::Cancelled);
        }
    };
    tandem_accumulate(&mut total, &step.usage);
    let display = strip_tandem_markers(&step.text);
    emit_tandem_step(
        tx,
        "🅒",
        critic_name,
        &format!("{} · {}", lang.choose("ревью", "review"), crit_role),
        &display,
    );
    transcript.push(crit_role, lang.choose("ревью", "review"), &display);
    let review_ok = step.code == 0 && parse_tandem_signal(&step.text);
    if step.code == 0 {
        emit_tandem_status(
            tx,
            if review_ok {
                lang.choose("✓ Ревью пройдено", "✓ Review passed")
            } else {
                lang.choose("↳ Есть правки — исправляю", "↳ Fixes needed — applying")
            },
        );
    }

    // ФИНАЛЬНАЯ ПРАВКА + ПОДТВЕРЖДЕНИЕ (P4)
    let mut leftover = false;
    if !review_ok {
        let review_text = step.text.clone();
        if cancel_rx.try_recv().is_ok() {
            dirty_notice(tx);
            return Ok(TandemResult::Cancelled);
        }
        let fix = tandem_fix_prompt(task, &transcript.render(), &review_text, lang);
        let step = match run_step(executor, executor_effort, &fix, RunAccess::PlanExecute)? {
            Some(s) => s,
            None => {
                dirty_notice(tx);
                return Ok(TandemResult::Cancelled);
            }
        };
        tandem_accumulate(&mut total, &step.usage);
        let display = strip_tandem_markers(&step.text);
        emit_tandem_step(
            tx,
            "🅐",
            executor_name,
            &format!(
                "{} · {}",
                lang.choose("финальная правка", "final fix"),
                exec_role
            ),
            &display,
        );
        transcript.push(
            exec_role,
            lang.choose("финальная правка", "final fix"),
            &display,
        );
        // Фаза правки — мутирующая (PlanExecute). Её ошибку/таймаут НЕ глотаем, как и
        // фазы дебатов/исполнения: иначе провалившийся прогон отдал бы код 0 (успех).
        if step.code != 0 {
            dirty_notice(tx);
            return Ok(TandemResult::Completed(step.code, opt_usage(total)));
        }

        let confirm = tandem_confirm_prompt(task, &transcript.render(), lang);
        let step = match run_step(critic, critic_effort, &confirm, RunAccess::PlanReadonly)? {
            Some(s) => s,
            None => {
                dirty_notice(tx);
                return Ok(TandemResult::Cancelled);
            }
        };
        tandem_accumulate(&mut total, &step.usage);
        let display = strip_tandem_markers(&step.text);
        emit_tandem_step(
            tx,
            "🅒",
            critic_name,
            &format!(
                "{} · {}",
                lang.choose("подтверждение", "confirmation"),
                crit_role
            ),
            &display,
        );
        // Провал самой фазы подтверждения (код≠0/таймаут) тоже не выдаём за успех.
        if step.code != 0 {
            dirty_notice(tx);
            return Ok(TandemResult::Completed(step.code, opt_usage(total)));
        }
        if !parse_tandem_signal(&step.text) {
            leftover = true;
            tandem_notice(
                tx,
                lang.choose(
                    "⚠ Остались замечания критика.",
                    "⚠ The critic still has unresolved issues.",
                )
                .to_string(),
            );
        }
    }

    // Человеческий итог тандема: что произошло, а не голый код возврата. Идёт последней
    // строкой ленты — пользователю сразу виден статус и прогресс.
    tandem_notice(
        tx,
        tandem_summary(rounds_done, consensus, review_ok, leftover, lang),
    );
    Ok(TandemResult::Completed(0, opt_usage(total)))
}

/// Однострочный человеческий итог успешного прогона тандема (код 0): консенсус за N раундов
/// или исполнение по решению пользователя; была ли правка после ревью и остались ли замечания.
fn tandem_summary(
    rounds_done: usize,
    consensus: bool,
    review_ok: bool,
    leftover: bool,
    lang: Language,
) -> String {
    let head = if consensus {
        format!(
            "{} {rounds_done} {}",
            lang.choose("✓ Тандем: консенсус за", "✓ Tandem: consensus in"),
            lang.choose("р.", "round(s)")
        )
    } else {
        lang.choose(
            "✓ Тандем: без консенсуса, исполнено по твоему решению",
            "✓ Tandem: no consensus, executed on your approval",
        )
        .to_string()
    };
    let tail = if leftover {
        lang.choose(
            " · правка внесена, но остались замечания",
            " · fix applied, issues remain",
        )
    } else if review_ok {
        lang.choose(" · исполнение подтверждено", " · execution confirmed")
    } else {
        lang.choose(" · исполнение с правкой", " · executed with a fix")
    };
    format!("{head}{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tandem_signal_parses_last_marker() {
        assert!(parse_tandem_signal("bla bla\nTANDEM: CONSENSUS"));
        assert!(!parse_tandem_signal("TANDEM: CONTINUE\nmore text"));
        assert!(!parse_tandem_signal("no signal here"));
        // последний маркер решает
        assert!(!parse_tandem_signal(
            "TANDEM: CONSENSUS\n...\nTANDEM: CONTINUE"
        ));
        // строгий разбор: упоминание TANDEM: в прозе — НЕ сигнал
        assert!(!parse_tandem_signal(
            "I will output TANDEM: CONSENSUS when we are done."
        ));
        // markdown-обрамление снимается, чистый сигнал проходит
        assert!(parse_tandem_signal("**TANDEM: CONSENSUS**"));
        assert!(parse_tandem_signal("> TANDEM: CONSENSUS"));
        // хвостовой текст после сигнала не засчитывается как консенсус
        assert!(!parse_tandem_signal("TANDEM: CONSENSUS reached, ship it"));
    }

    #[test]
    fn tandem_transcript_renders_and_truncates() {
        let mut t = TandemTranscript::new();
        t.push("Executor", "proposal 1", "short");
        assert!(t.render().contains("short"));
        for i in 0..60 {
            t.push("Critic", "round", &format!("entry {i} {}", "y".repeat(400)));
        }
        assert!(t.render().contains("усечены"));
    }

    #[test]
    fn tandem_accumulate_sums_every_field() {
        let mut total = RunUsage::default();
        let first = RunUsage {
            input: 10,
            output: 2,
            cache_read: 3,
            cache_creation: 4,
            cost_usd: 0.5,
        };
        tandem_accumulate(&mut total, &Some(first));
        assert_eq!(total, first);

        // Второй шаг ПРИБАВЛЯЕТСЯ (разные значения — перестановка полей заметна).
        tandem_accumulate(
            &mut total,
            &Some(RunUsage {
                input: 1,
                output: 20,
                cache_read: 300,
                cache_creation: 4000,
                cost_usd: 0.25,
            }),
        );
        assert_eq!(total.input, 11);
        assert_eq!(total.output, 22);
        assert_eq!(total.cache_read, 303);
        assert_eq!(total.cache_creation, 4004);
        assert!((total.cost_usd - 0.75).abs() < 1e-9, "{}", total.cost_usd);

        // Шаг без usage (codex без токенов) ничего не портит.
        tandem_accumulate(&mut total, &None);
        assert_eq!(total.input, 11);
        assert_eq!(total.output, 22);
    }

    #[test]
    fn opt_usage_hides_only_the_empty_total() {
        assert!(opt_usage(RunUsage::default()).is_none());
        let some = RunUsage {
            input: 1,
            ..RunUsage::default()
        };
        assert_eq!(opt_usage(some), Some(some));
    }

    #[test]
    fn emit_tandem_step_streams_header_body_then_commit_signal() {
        let (tx, rx) = mpsc::channel();
        emit_tandem_step(
            &tx,
            "🅐",
            "Claude",
            "раунд 1 · Исполнитель",
            "  первая\nвторая  ",
        );
        drop(tx);
        let events: Vec<WorkerEvent> = rx.iter().collect();
        let lines: Vec<&str> = events
            .iter()
            .filter_map(|event| match event {
                WorkerEvent::ChatLine(line) => Some(line.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            lines,
            ["", "🅐 Claude · раунд 1 · Исполнитель", "первая", "вторая"],
            "разделитель ПЕРЕД шагом, заголовок, затем тело"
        );
        // Шаг завершается сигналом «зафиксировать в ленте сразу».
        assert!(
            matches!(events.last(), Some(WorkerEvent::TandemStepEnd)),
            "последним идёт TandemStepEnd: {events:?}"
        );
    }

    #[test]
    fn tandem_notice_goes_to_the_status_line() {
        let (tx, rx) = mpsc::channel();
        tandem_notice(&tx, "⚠ внимание".to_string());
        match rx.try_recv() {
            Ok(WorkerEvent::Line(line)) => assert_eq!(line, "⚠ внимание"),
            _ => panic!("уведомление не отправлено"),
        }
    }

    #[test]
    fn tandem_transcript_keeps_everything_while_short() {
        // Много КОРОТКИХ записей — усечения нет (лимит по объёму, а не по числу).
        let mut t = TandemTranscript::new();
        for i in 1..=6 {
            t.push("Критик", "раунд", &format!("запись {i}"));
        }
        let render = t.render();
        assert!(!render.contains("усечены"));
        assert!(render.contains("запись 2"));

        // Четыре ДЛИННЫЕ записи — тоже целиком: обрезать нечего, это первый круг.
        let mut big = TandemTranscript::new();
        for i in 1..=4 {
            big.push(
                "Исполнитель",
                "раунд",
                &format!("запись {i} {}", "x".repeat(4000)),
            );
        }
        assert!(!big.render().contains("усечены"));
    }

    #[test]
    fn tandem_transcript_truncation_keeps_head_and_last_three() {
        let mut t = TandemTranscript::new();
        for i in 1..=5 {
            t.push(
                "Исполнитель",
                "раунд",
                &format!("запись {i} {}", "x".repeat(4000)),
            );
        }
        let render = t.render();
        assert!(render.contains("усечены"));
        assert!(
            render.contains("запись 1"),
            "первая запись — задача — остаётся"
        );
        assert!(!render.contains("запись 2"), "ранние раунды выброшены");
        for i in 3..=5 {
            assert!(
                render.contains(&format!("запись {i}")),
                "хвост — последние три"
            );
        }
    }

    // --- решение о повторе в чате ---

    fn step(text: &str) -> Option<TandemStep> {
        Some(TandemStep {
            text: text.to_string(),
            code: 0,
            usage: None,
        })
    }

    fn failing_step(code: i32) -> Option<TandemStep> {
        Some(TandemStep {
            text: "сбой".to_string(),
            code,
            usage: None,
        })
    }

    struct TandemRun {
        result: TandemResult,
        calls: Vec<(&'static str, RunAccess)>,
        notices: Vec<String>,
        chat: Vec<String>,
        needs_approval: bool,
        needs_input: bool,
        needs_choice: bool,
    }

    /// Прогоняет оркестратор на СЦЕНАРИИ шагов (None = отмена внутри шага).
    /// `cancel_after` — послать отмену в канал после N-го шага. Решение гейта «нет
    /// консенсуса» по умолчанию — `Execute` (путь «нет консенсуса → исполнение»).
    fn fake_tandem(
        steps: Vec<Option<TandemStep>>,
        rounds: usize,
        cancel_after: Option<usize>,
    ) -> TandemRun {
        fake_tandem_full(steps, rounds, cancel_after, TandemGate::Execute, &[])
    }

    /// То же, но с явным решением гейта «нет консенсуса».
    fn fake_tandem_gated(
        steps: Vec<Option<TandemStep>>,
        rounds: usize,
        cancel_after: Option<usize>,
        gate: TandemGate,
    ) -> TandemRun {
        fake_tandem_full(steps, rounds, cancel_after, gate, &[])
    }

    /// Полный харнесс: `inputs` — ответы пользователя на ввод-гейт `NEED_INPUT`,
    /// предзагружаются по порядку; затем канал закрывается, поэтому лишний `NEED_INPUT`
    /// без ответа даёт Disconnected → воркер идёт дальше (как headless), а не виснет.
    fn fake_tandem_full(
        steps: Vec<Option<TandemStep>>,
        rounds: usize,
        cancel_after: Option<usize>,
        gate: TandemGate,
        inputs: &[&str],
    ) -> TandemRun {
        let (tx, rx) = mpsc::channel();
        let (cancel_tx, cancel_rx) = mpsc::channel();
        let (gate_tx, gate_rx) = mpsc::channel();
        gate_tx.send(gate).expect("решение гейта уходит в канал");
        let (input_tx, input_rx) = mpsc::channel();
        for answer in inputs {
            input_tx.send(answer.to_string()).expect("ответ в канал");
        }
        drop(input_tx); // ответы исчерпаны → Disconnected, без зависания
        let mut calls = Vec::new();
        let mut scripted = steps.into_iter();
        let mut done = 0usize;

        let result = run_tandem_with(
            |provider, _effort, _prompt, access| {
                calls.push((provider, access));
                done += 1;
                if cancel_after == Some(done) {
                    cancel_tx.send(()).expect("отмена уходит в канал");
                }
                Ok(scripted.next().flatten())
            },
            "claude",
            "codex",
            "max",
            "xhigh",
            "задача",
            rounds,
            &cancel_rx,
            &gate_rx,
            &input_rx,
            &tx,
            Language::Ru,
        )
        .expect("оркестратор не падает");

        drop(tx);
        let mut notices = Vec::new();
        let mut chat = Vec::new();
        let mut needs_approval = false;
        let mut needs_input = false;
        let mut needs_choice = false;
        for event in rx.iter() {
            match event {
                WorkerEvent::Line(line) => notices.push(line),
                WorkerEvent::ChatLine(line) => chat.push(line),
                WorkerEvent::TandemNeedsApproval => needs_approval = true,
                WorkerEvent::TandemNeedsInput(prompt) => {
                    needs_input = true;
                    needs_choice = prompt.is_some();
                }
                _ => {}
            }
        }
        TandemRun {
            result,
            calls,
            notices,
            chat,
            needs_approval,
            needs_input,
            needs_choice,
        }
    }

    #[test]
    fn tandem_reports_a_failing_fix_phase_instead_of_success() {
        // Дебаты→консенсус→исполнение→ревью с замечаниями→ФАЗА ПРАВКИ падает (code 7).
        // Раньше код фазы правки игнорировался и тандем отдавал 0 (успех); теперь — её код.
        let run = fake_tandem(
            vec![
                step("предложение"),
                step("TANDEM: CONSENSUS"),
                step("сделал"),
                step("есть замечания"), // ревью без консенсуса → review_ok=false → фаза правки
                failing_step(7),        // финальная правка упала
            ],
            3,
            None,
        );
        match run.result {
            TandemResult::Completed(code, _) => {
                assert_eq!(code, 7, "провал фазы правки отдаётся кодом, а не 0")
            }
            TandemResult::Cancelled => panic!("не отменяли — правка провалилась"),
        }
    }

    #[test]
    fn tandem_stops_at_consensus_and_scopes_access_per_phase() {
        let run = fake_tandem(
            vec![
                step("предложение"),
                step("TANDEM: CONSENSUS"),
                step("сделал"),
                step("ревью ок\nTANDEM: CONSENSUS"),
            ],
            3,
            None,
        );

        // Дебаты и ревью — только чтение; правки разрешены ТОЛЬКО в исполнении.
        assert_eq!(
            run.calls,
            vec![
                ("claude", RunAccess::PlanReadonly),
                ("codex", RunAccess::PlanReadonly),
                ("claude", RunAccess::PlanExecute),
                ("codex", RunAccess::PlanReadonly),
            ],
            "консенсус в раунде 1 → второго раунда нет, правка не нужна"
        );
        match run.result {
            TandemResult::Completed(code, usage) => {
                assert_eq!(code, 0);
                assert!(
                    usage.is_none(),
                    "шаги без usage → пустой итог не показываем"
                );
            }
            TandemResult::Cancelled => panic!("не отменяли"),
        }
        // Успех даёт человеческий итог (а не голый код) и НИ одного предупреждения.
        assert!(
            run.notices
                .iter()
                .any(|l| l.contains("✓ Тандем") && l.contains("консенсус")),
            "успешный тандем даёт человеческий итог: {:?}",
            run.notices
        );
        assert!(
            !run.notices.iter().any(|l| l.contains('⚠')),
            "предупреждений быть не должно: {:?}",
            run.notices
        );
        // Сырой протокольный маркер спрятан, вместо него — человеческий статус.
        assert!(
            !run.chat.iter().any(|l| l.contains("TANDEM: CONSENSUS")),
            "сырой TANDEM: CONSENSUS не должен течь в ленту: {:?}",
            run.chat
        );
        assert!(
            run.chat.iter().any(|l| l.contains("✓ Консенсус")),
            "вместо маркера — человеческий статус: {:?}",
            run.chat
        );
        assert!(
            run.chat
                .iter()
                .any(|l| l == "🅐 Claude · раунд 1 · Исполнитель"),
            "шаги стримятся в чат: {:?}",
            run.chat
        );
        assert!(run.chat.iter().any(|l| l.contains("ревью")));
    }

    #[test]
    fn tandem_gates_execution_when_rounds_end_without_consensus() {
        // Fix B: нет консенсуса → воркер НЕ пишет молча. Он эмитит запрос одобрения и
        // блокируется. С предзагруженным Execute (дефолт) — проходит в исполнение и ревью.
        let approved = fake_tandem(
            vec![
                step("предложение 1"),
                step("TANDEM: CONTINUE"),
                step("предложение 2"),
                step("TANDEM: CONTINUE"),
                step("сделал"),
                step("TANDEM: CONSENSUS"),
            ],
            2,
            None,
        );
        assert!(
            approved.needs_approval,
            "нет консенсуса → обязан запросить одобрение, а не писать молча"
        );
        assert_eq!(
            approved.calls.len(),
            6,
            "одобрено → оба раунда дебатов + исполнение + ревью"
        );
        assert_eq!(
            approved.calls[4],
            ("claude", RunAccess::PlanExecute),
            "5-й вызов — исполнение с доступом на запись, но ТОЛЬКО после одобрения"
        );

        // Esc/Abort на гейте: исполнение не запускается, файлы не тронуты, ран отменён.
        let aborted = fake_tandem_gated(
            vec![
                step("предложение 1"),
                step("TANDEM: CONTINUE"),
                step("предложение 2"),
                step("TANDEM: CONTINUE"),
                step("сделал"), // недостижимо: гейт отклонён до исполнения
                step("TANDEM: CONSENSUS"),
            ],
            2,
            None,
            TandemGate::Abort,
        );
        assert!(aborted.needs_approval, "гейт показан и при отказе");
        assert_eq!(
            aborted.calls.len(),
            4,
            "отказ на гейте → только дебаты; фаза исполнения не запускалась"
        );
        assert!(
            matches!(aborted.result, TandemResult::Cancelled),
            "отказ = отмена без записи"
        );
        assert!(
            !aborted
                .notices
                .iter()
                .any(|l| l.contains("Файлы были изменены")),
            "до исполнения файлы не трогали: {:?}",
            aborted.notices
        );
    }

    #[test]
    fn tandem_fixes_after_review_and_reports_leftovers() {
        let unresolved = fake_tandem(
            vec![
                step("предложение"),
                step("TANDEM: CONSENSUS"),
                step("сделал"),
                step("TANDEM: CONTINUE — течёт память"),
                step("починил"),
                step("TANDEM: CONTINUE"),
            ],
            1,
            None,
        );
        assert_eq!(
            unresolved.calls[4..],
            [
                ("claude", RunAccess::PlanExecute),
                ("codex", RunAccess::PlanReadonly)
            ],
            "правка исполнителя (с доступом на запись) + подтверждение критика"
        );
        assert!(
            unresolved
                .notices
                .iter()
                .any(|line| line.contains("Остались замечания")),
            "{:?}",
            unresolved.notices
        );

        // Подтверждение получено — предупреждения нет.
        let resolved = fake_tandem(
            vec![
                step("предложение"),
                step("TANDEM: CONSENSUS"),
                step("сделал"),
                step("TANDEM: CONTINUE — течёт память"),
                step("починил"),
                step("TANDEM: CONSENSUS"),
            ],
            1,
            None,
        );
        assert_eq!(resolved.calls.len(), 6);
        // Подтверждение получено → «остались замечания» НЕ выводим, но человеческий итог есть.
        assert!(
            !resolved
                .notices
                .iter()
                .any(|line| line.contains("Остались замечания")),
            "замечаний не осталось: {:?}",
            resolved.notices
        );
        assert!(
            resolved.notices.iter().any(|l| l.contains("✓ Тандем")),
            "успех даёт человеческий итог: {:?}",
            resolved.notices
        );
    }

    #[test]
    fn tandem_marker_helpers_detect_and_strip() {
        // NEED_INPUT — только когда это ПОСЛЕДНИЙ значимый сигнал.
        assert!(tandem_needs_input("вопросы к тебе\nTANDEM: NEED_INPUT"));
        assert!(!tandem_needs_input("TANDEM: CONSENSUS"));
        assert!(!tandem_needs_input("обычный ответ без сигнала"));

        // Стрип: чистый маркер уходит целиком, человеческий хвост возражений сохраняется.
        assert_eq!(strip_tandem_markers("план\nTANDEM: CONSENSUS"), "план");
        assert_eq!(
            strip_tandem_markers("критика\nTANDEM: CONTINUE — течёт память"),
            "критика\nтечёт память"
        );
        assert_eq!(strip_tandem_markers("**TANDEM: CONSENSUS**"), "");
        assert_eq!(strip_tandem_markers("обычный ответ"), "обычный ответ");

        // Выдуманный моделью маркер тоже прячем (со своим хвостом) — не только три известных.
        assert_eq!(strip_tandem_markers("итог\nTANDEM: CLOSED"), "итог");
        assert_eq!(
            strip_tandem_markers("правка\nTANDEM: DONE — всё готово"),
            "правка\nвсё готово"
        );
        // Но детекция СИГНАЛА строгая: выдуманный CLOSED — это НЕ консенсус.
        assert!(!parse_tandem_signal("бла\nTANDEM: CLOSED"));
        assert!(!tandem_needs_input("бла\nTANDEM: CLOSED"));
    }

    #[test]
    fn tandem_pauses_for_input_then_resumes_with_answer() {
        // Раунд 1: исполнитель просит уточнений (NEED_INPUT). Пользователь отвечает →
        // исполнитель ПЕРЕ-предлагает (уже с ответом в ленте), критик даёт консенсус.
        let run = fake_tandem_full(
            vec![
                step("Задача не задана. Вопросы:\nTANDEM: NEED_INPUT"),
                step("Теперь понятно — предлагаю X"),
                step("TANDEM: CONSENSUS"),
                step("сделал"),
                step("ревью ок\nTANDEM: CONSENSUS"),
            ],
            2,
            None,
            TandemGate::Execute,
            &["Задача: почини баг в X"],
        );
        assert!(
            run.needs_input,
            "исполнитель запросил ввод → событие поднято"
        );
        assert_eq!(
            run.calls.len(),
            5,
            "предложение(need_input) + пере-предложение + критик + исполнение + ревью: {:?}",
            run.calls
        );
        assert_eq!(run.calls[0], ("claude", RunAccess::PlanReadonly));
        assert_eq!(
            run.calls[1],
            ("claude", RunAccess::PlanReadonly),
            "повтор — тоже дебаты"
        );
        // Сырой NEED_INPUT спрятан, вместо него человеческий статус.
        assert!(
            !run.chat.iter().any(|l| l.contains("TANDEM: NEED_INPUT")),
            "маркер не течёт в ленту: {:?}",
            run.chat
        );
        assert!(
            run.chat.iter().any(|l| l.contains("Нужны уточнения")),
            "человеческий статус запроса: {:?}",
            run.chat
        );
        match run.result {
            TandemResult::Completed(code, _) => assert_eq!(code, 0),
            TandemResult::Cancelled => panic!("не отменяли"),
        }
    }

    #[test]
    fn tandem_offers_choice_selector_for_closed_questions() {
        // Закрытый вопрос: исполнитель прикладывает блок ```clave-ask + NEED_INPUT. Событие
        // несёт AskPrompt (→ селектор), проза показана, а JSON-блок НЕ течёт в ленту.
        let block = "Какую задачу решаем?\n\
             ```clave-ask\n\
             {\"question\":\"Тип?\",\"multi\":false,\"options\":[{\"label\":\"фича\"},{\"label\":\"багфикс\"}]}\n\
             ```\n\
             TANDEM: NEED_INPUT";
        let run = fake_tandem_full(
            vec![
                step(block),
                step("Понял — предлагаю X"),
                step("TANDEM: CONSENSUS"),
                step("сделал"),
                step("ревью\nTANDEM: CONSENSUS"),
            ],
            2,
            None,
            TandemGate::Execute,
            &["Выбрано: «багфикс»"],
        );
        assert!(run.needs_input, "запрос ввода поднят");
        assert!(
            run.needs_choice,
            "закрытый вопрос → в событии AskPrompt (селектор)"
        );
        assert!(
            !run.chat
                .iter()
                .any(|l| l.contains("clave-ask") || l.contains("\"options\"")),
            "JSON-блок выбора не течёт в ленту: {:?}",
            run.chat
        );
        assert!(
            run.chat.iter().any(|l| l.contains("Какую задачу решаем")),
            "проза вопроса показана: {:?}",
            run.chat
        );
        assert!(matches!(run.result, TandemResult::Completed(0, _)));
    }

    #[test]
    fn tandem_need_input_without_answerer_proceeds_not_hangs() {
        // Нет ответчика (headless): NEED_INPUT → канал закрыт → воркер идёт к критику,
        // а не виснет навсегда. Завершение теста и есть доказательство отсутствия зависания.
        let run = fake_tandem_full(
            vec![
                step("Вопросы:\nTANDEM: NEED_INPUT"),
                step("TANDEM: CONTINUE"),
                step("сделал"),
                step("ревью\nTANDEM: CONSENSUS"),
            ],
            1,
            None,
            TandemGate::Execute,
            &[], // ответов нет → Disconnected → идём дальше
        );
        assert!(run.needs_input, "запрос ввода поднят даже без ответчика");
        assert_eq!(
            run.calls.len(),
            4,
            "предложение(need_input→дальше) + критик + исполнение + ревью: {:?}",
            run.calls
        );
        assert!(matches!(run.result, TandemResult::Completed(0, _)));
    }

    #[test]
    fn tandem_review_with_nonzero_code_is_not_consensus() {
        // Ревью упало с ошибкой, но текст содержит сигнал — доверять ему нельзя.
        let run = fake_tandem(
            vec![
                step("предложение"),
                step("TANDEM: CONSENSUS"),
                step("сделал"),
                Some(TandemStep {
                    text: "TANDEM: CONSENSUS".to_string(),
                    code: 2,
                    usage: None,
                }),
                step("починил"),
                step("TANDEM: CONSENSUS"),
            ],
            1,
            None,
        );
        assert_eq!(run.calls.len(), 6, "правка всё равно выполняется");
    }

    #[test]
    fn tandem_stops_on_provider_error_in_each_phase() {
        // Ошибка исполнителя в дебатах: дальше не идём, код возврата — провайдера.
        let debate = fake_tandem(vec![failing_step(7)], 2, None);
        assert_eq!(debate.calls.len(), 1);
        match debate.result {
            TandemResult::Completed(code, _) => assert_eq!(code, 7),
            TandemResult::Cancelled => panic!("не отменяли"),
        }
        assert!(
            debate
                .notices
                .iter()
                .any(|line| line.contains("Claude") && line.contains("ошибку")),
            "{:?}",
            debate.notices
        );
        assert!(
            !debate
                .notices
                .iter()
                .any(|line| line.contains("Файлы были изменены")),
            "до исполнения файлы не трогали"
        );

        // Ошибка критика в дебатах.
        let critic = fake_tandem(vec![step("предложение"), failing_step(5)], 2, None);
        assert_eq!(critic.calls.len(), 2);
        match critic.result {
            TandemResult::Completed(code, _) => assert_eq!(code, 5),
            TandemResult::Cancelled => panic!("не отменяли"),
        }
        assert!(critic
            .notices
            .iter()
            .any(|line| line.contains("Codex") && line.contains("ошибку")));

        // Ошибка на ИСПОЛНЕНИИ: файлы уже могли измениться — предупреждаем.
        let execute = fake_tandem(
            vec![
                Some(TandemStep {
                    text: "предложение".to_string(),
                    code: 0,
                    usage: Some(RunUsage {
                        input: 5,
                        ..RunUsage::default()
                    }),
                }),
                step("TANDEM: CONSENSUS"),
                Some(TandemStep {
                    text: "упал".to_string(),
                    code: 9,
                    usage: Some(RunUsage {
                        input: 2,
                        ..RunUsage::default()
                    }),
                }),
            ],
            1,
            None,
        );
        assert_eq!(execute.calls.len(), 3);
        match execute.result {
            TandemResult::Completed(code, usage) => {
                assert_eq!(code, 9);
                assert_eq!(
                    usage.expect("usage суммируется даже при сбое").input,
                    7,
                    "5 + 2 — токены обоих шагов"
                );
            }
            TandemResult::Cancelled => panic!("не отменяли"),
        }
        assert!(
            execute
                .notices
                .iter()
                .any(|line| line.contains("Файлы были изменены")),
            "{:?}",
            execute.notices
        );
    }

    #[test]
    fn tandem_surfaces_step_output_before_reporting_an_error() {
        // Регресс Fix A: при code≠0 вывод шага раньше ГЛОТАЛСЯ (emit шёл после return),
        // и пользователь видел только «вернул ошибку» — причина (в самом выводе провайдера)
        // пропадала, отлаживать было нечего. Теперь текст падающего шага обязан уйти в чат
        // ДО обработки кода — в каждой мутирующей/дебатной фазе.
        let has = |chat: &[String], needle: &str| chat.iter().any(|l| l.contains(needle));

        // Дебаты — исполнитель падает, причина в его выводе.
        let debate_exec = fake_tandem(
            vec![Some(TandemStep {
                text: "не нашёл модуль loader".to_string(),
                code: 1,
                usage: None,
            })],
            2,
            None,
        );
        assert!(
            has(&debate_exec.chat, "не нашёл модуль loader"),
            "вывод падающего исполнителя в дебатах должен попасть в чат: {:?}",
            debate_exec.chat
        );

        // Дебаты — критик падает, причина в его выводе.
        let debate_crit = fake_tandem(
            vec![
                step("предложение"),
                Some(TandemStep {
                    text: "критик: таймаут провайдера".to_string(),
                    code: 1,
                    usage: None,
                }),
            ],
            2,
            None,
        );
        assert!(
            has(&debate_crit.chat, "критик: таймаут провайдера"),
            "вывод падающего критика в дебатах должен попасть в чат: {:?}",
            debate_crit.chat
        );

        // Исполнение падает с кодом 1 (ровно кейс реального прогона): вывод исполнителя —
        // единственная диагностика, и он обязан быть виден до «вернул ошибку».
        let execute = fake_tandem(
            vec![
                step("предложение"),
                step("TANDEM: CONSENSUS"),
                Some(TandemStep {
                    text: "написал ключ-гард, но упал на сборке".to_string(),
                    code: 1,
                    usage: None,
                }),
            ],
            1,
            None,
        );
        assert!(
            has(&execute.chat, "написал ключ-гард, но упал на сборке"),
            "вывод падающего исполнения должен попасть в чат: {:?}",
            execute.chat
        );
    }

    #[test]
    fn tandem_cancellation_warns_only_after_files_could_change() {
        // Отмена внутри шага дебатов — файлы ещё чистые.
        let debate = fake_tandem(vec![None], 2, None);
        assert!(matches!(debate.result, TandemResult::Cancelled));
        assert!(debate.notices.is_empty(), "{:?}", debate.notices);

        // Отмена ПОСЛЕ дебатов (перед исполнением) — тоже без правок.
        let before_execute = fake_tandem(
            vec![step("предложение"), step("TANDEM: CONSENSUS")],
            1,
            Some(2),
        );
        assert!(matches!(before_execute.result, TandemResult::Cancelled));
        assert_eq!(before_execute.calls.len(), 2, "исполнение не запускалось");
        assert!(before_execute.notices.is_empty());

        // Отмена внутри ИСПОЛНЕНИЯ — предупреждаем о возможных правках.
        let during_execute = fake_tandem(
            vec![step("предложение"), step("TANDEM: CONSENSUS"), None],
            1,
            None,
        );
        assert!(matches!(during_execute.result, TandemResult::Cancelled));
        assert!(during_execute
            .notices
            .iter()
            .any(|line| line.contains("Файлы были изменены")));

        // Отмена между ревью и правкой — файлы уже изменены.
        let before_fix = fake_tandem(
            vec![
                step("предложение"),
                step("TANDEM: CONSENSUS"),
                step("сделал"),
                step("TANDEM: CONTINUE"),
            ],
            1,
            Some(4),
        );
        assert!(matches!(before_fix.result, TandemResult::Cancelled));
        assert_eq!(before_fix.calls.len(), 4, "правка не запускалась");
        assert!(before_fix
            .notices
            .iter()
            .any(|line| line.contains("Файлы были изменены")));

        // Отмена внутри ревью / правки / подтверждения — тот же режим.
        let during_review = fake_tandem(
            vec![
                step("предложение"),
                step("TANDEM: CONSENSUS"),
                step("сделал"),
                None,
            ],
            1,
            None,
        );
        assert!(matches!(during_review.result, TandemResult::Cancelled));
        assert!(during_review
            .notices
            .iter()
            .any(|line| line.contains("Файлы были изменены")));
    }
}
