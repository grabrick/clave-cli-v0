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
pub(crate) fn strip_tandem_markers(text: &str) -> String {
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
}
