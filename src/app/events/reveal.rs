use super::*;

/// Плавная «печатная машинка» для ответа: целиком готовый текст вскрывается
/// по символам со временем, пока полностью не уйдёт в историю.
pub(crate) struct Reveal {
    pub(crate) text: String,
    pub(crate) shown: usize,
    pub(crate) started: Instant,
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
    pub(crate) fn commit_answer_text(&mut self, prose: &str) {
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
    pub(crate) fn flush_reveal_buffer(&mut self) {
        for line in std::mem::take(&mut self.reveal_buffer) {
            self.push_system(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::events::testkit::*;

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
