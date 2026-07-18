use crate::prelude::*;
use crate::*;

pub(crate) fn tandem_accumulate(total: &mut RunUsage, usage: &Option<RunUsage>) {
    if let Some(u) = usage {
        total.input += u.input;
        total.output += u.output;
        total.cache_read += u.cache_read;
        total.cache_creation += u.cache_creation;
        total.cost_usd += u.cost_usd;
    }
}

pub(crate) fn emit_tandem_step(
    tx: &Sender<WorkerEvent>,
    marker: &str,
    who: &str,
    phase: &str,
    text: &str,
) {
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

pub(crate) fn tandem_notice(tx: &Sender<WorkerEvent>, text: String) {
    let _ = tx.send(WorkerEvent::Line(text));
}

/// Человеческая статус-строка в ленту (вместо сырого `TANDEM: …`): «✓ Консенсус» и т.п.
/// Идёт отдельной строкой после шага, с отступом — как продолжение блока шага.
pub(crate) fn emit_tandem_status(tx: &Sender<WorkerEvent>, text: &str) {
    let _ = tx.send(WorkerEvent::ChatLine(format!("  {text}")));
    let _ = tx.send(WorkerEvent::TandemStepEnd);
}

pub(crate) fn opt_usage(total: RunUsage) -> Option<RunUsage> {
    if total == RunUsage::default() {
        None
    } else {
        Some(total)
    }
}

/// Однострочный человеческий итог успешного прогона тандема (код 0): консенсус за N раундов
/// или исполнение по решению пользователя; была ли правка после ревью и остались ли замечания.
pub(crate) fn tandem_summary(
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
}
