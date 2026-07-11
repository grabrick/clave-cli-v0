use super::*;

pub(crate) fn loader_line(app: &App) -> Line<'static> {
    let elapsed = app
        .run_started_at
        .map(|started| started.elapsed())
        .unwrap_or_else(|| Duration::from_secs(0));
    // Фраза отражает РЕАЛЬНОЕ состояние выполнения, а не крутится по таймеру
    // (иначе сразу «видно, что это скрипт»): рассуждение → пишет ответ → ждёт.
    // Конкретику (какой файл читает и т.п.) дают строки активности ⎿ ниже.
    let reasoning = !app.live_reasoning.is_empty() && app.live_answer.is_empty();
    let answering = !app.live_answer.is_empty();
    // Шиммер включаем только когда реально идёт работа: стрим рассуждения/ответа
    // или активность инструментов. В тишине ожидания первого байта — статично.
    let active = reasoning || answering || !app.run_activity.is_empty();
    let phrase = if reasoning {
        app.lang.choose("Рассуждаю", "Reasoning")
    } else if answering {
        app.lang.choose("Пишу ответ", "Writing answer")
    } else {
        app.lang.choose("Думаю", "Thinking")
    };
    let label = if app.run_label.is_empty() {
        app.mode.as_str().to_string()
    } else {
        app.run_label.clone()
    };
    // Живая оценка расхода: реально отправленный промт (run_token_estimate) плюс
    // уже принятый текст ответа (live_answer растёт по токенам у claude). Цифра
    // приблизительная (≈, токенизация по символам), но опирается на реальный
    // текст и растёт по факту. Точный usage·$ — в футере по завершении.
    let out_tokens = if app.live_answer.is_empty() {
        0
    } else {
        estimate_tokens(&app.live_answer)
    };
    let tokens = app.run_token_estimate.unwrap_or(0) + out_tokens;
    let detail = if tokens > 0 {
        format!(
            "({} · {} · ≈ {} {})",
            format_elapsed(elapsed),
            label,
            format_token_count(tokens),
            app.lang.choose("токенов", "tokens"),
        )
    } else {
        format!("({} · {})", format_elapsed(elapsed), label)
    };

    let head = format!("✳ {phrase}… ");
    let mut spans = if active {
        theme_shimmer_text_spans(&head, app.theme, current_effort_tick())
    } else {
        // Тишина ожидания — статичная приглушённая фраза без переливов.
        vec![Span::styled(
            head,
            Style::default()
                .fg(app.theme.accent_dim())
                .add_modifier(Modifier::BOLD),
        )]
    };
    spans.push(Span::styled(
        detail,
        Style::default().fg(Color::Indexed(245)),
    ));
    Line::from(spans)
}

/// Глагол прошедшего времени для «замороженного» лоадера. Детерминирован по
/// seed (длительность рана) — выбирается один раз и не мигает между кадрами.
pub(crate) fn idle_verb(lang: Language, seed: u128) -> &'static str {
    let ru: [&'static str; 5] = ["Думал", "Размышлял", "Соображал", "Кумекал", "Прикидывал"];
    let en: [&'static str; 5] = ["Thought", "Pondered", "Cogitated", "Mused", "Reasoned"];
    let verbs = match lang {
        Language::Ru => ru,
        Language::En => en,
    };
    verbs[(seed as usize) % verbs.len()]
}

/// Неактивная строка лоадера после завершения рана: `✻ {глагол} · {время}`.
/// Серая, без шиммера — в отличие от активной `✳ …`.
pub(crate) fn idle_loader_line(app: &App, elapsed: Duration) -> Line<'static> {
    let verb = idle_verb(app.lang, elapsed.as_millis());
    let text = format!("✻ {verb} · {}", format_elapsed(elapsed));
    Line::from(Span::styled(text, Style::default().fg(MUTED)))
}

pub(crate) fn loader_lines(app: &App, width: u16) -> Vec<Line<'static>> {
    let mut lines = vec![loader_line(app)];
    // Живой кусочек мысли, пока модель рассуждает до ответа — чтобы было видно,
    // что инструмент думает, а не просто крутит спиннер.
    if !app.live_reasoning.is_empty() && app.live_answer.is_empty() {
        if let Some(snippet) = reasoning_snippet(&app.live_reasoning, width) {
            lines.push(Line::from(vec![
                Span::styled("  ⎿ ", Style::default().fg(app.theme.accent_dim())),
                Span::styled(
                    snippet,
                    Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
                ),
            ]));
        }
    }
    lines.extend(loader_activity_lines(app, width));
    lines
}

/// Хвост текущей «мысли» из потока рассуждения: последняя непустая строка,
/// показанная с конца (самое свежее) и обрезанная по ширине.
fn reasoning_snippet(reasoning: &str, width: u16) -> Option<String> {
    let cap = width.saturating_sub(5).max(8) as usize;
    let last = reasoning
        .split('\n')
        .map(str::trim)
        .rfind(|line| !line.is_empty())?;
    let chars: Vec<char> = last.chars().collect();
    if chars.len() > cap {
        Some(format!(
            "…{}",
            chars[chars.len() - cap + 1..].iter().collect::<String>()
        ))
    } else {
        Some(last.to_string())
    }
}

pub(crate) fn loader_activity_lines(app: &App, width: u16) -> Vec<Line<'static>> {
    let content_width = width.saturating_sub(5).max(1) as usize;
    // По ОДНОЙ строке на активность и не более трёх последних: высота loader
    // должна быть предсказуемой. Иначе при каждом апдейте активности менялась бы
    // высота viewport, а его смена в inline-режиме = пересоздание терминала
    // (скролл-дрожь во время прогона).
    const MAX_ACTIVITY_LINES: usize = 3;
    let skip = app.run_activity.len().saturating_sub(MAX_ACTIVITY_LINES);
    app.run_activity
        .iter()
        .skip(skip)
        .map(|activity| {
            Line::from(vec![
                Span::styled("  ⎿ ", Style::default().fg(app.theme.accent_dim())),
                Span::styled(
                    truncate_chars(activity, content_width),
                    Style::default().fg(Color::Indexed(245)),
                ),
            ])
        })
        .collect()
}

pub(crate) fn theme_shimmer_text_spans(text: &str, theme: Theme, tick: u64) -> Vec<Span<'static>> {
    text.chars()
        .enumerate()
        .map(|(index, ch)| {
            Span::styled(
                ch.to_string(),
                Style::default()
                    .fg(theme_shimmer_color(theme, index, tick))
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect()
}

pub(crate) fn theme_shimmer_color(theme: Theme, index: usize, tick: u64) -> Color {
    let palette = [
        theme.accent_dim(),
        theme.accent(),
        theme.accent_soft(),
        theme.accent(),
        theme.accent_dim(),
    ];
    let phase = (tick as usize) % palette.len();
    let color_index = (index + palette.len() - phase) % palette.len();
    palette[color_index]
}

pub(crate) fn format_elapsed(duration: Duration) -> String {
    let total = duration.as_secs();
    if total < 60 {
        return format!("{}s", total.max(1));
    }

    let minutes = total / 60;
    let seconds = total % 60;
    if minutes < 60 {
        return format!("{}m {:02}s", minutes, seconds);
    }

    let hours = minutes / 60;
    let minutes = minutes % 60;
    format!("{}h {:02}m", hours, minutes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn loader_shows_token_estimate_when_known() {
        let mut app = App::new();
        app.run_started_at = Some(Instant::now());
        app.run_label = "Claude".to_string();
        app.run_token_estimate = Some(1200);
        // Поток ответа пуст — показываем оценку промта.
        let text = line_text(&loader_line(&app));
        assert!(text.contains('≈'), "есть пометка оценки: {text}");
        assert!(text.contains("1.2k"), "форматированный счётчик: {text}");
        assert!(text.contains("токенов"), "подпись по-русски: {text}");

        // Без оценки — старый вид без счётчика.
        app.run_token_estimate = None;
        app.live_answer.clear();
        let text = line_text(&loader_line(&app));
        assert!(!text.contains('≈'), "нет токенов — нет пометки: {text}");
    }

    #[test]
    fn loader_surfaces_reasoning_until_answer_starts() {
        let mut app = App::new();
        app.run_started_at = Some(Instant::now());
        app.live_reasoning = "сначала пойму задачу\nтеперь сверю с файлами".to_string();
        // Пока ответа нет — лоадер прямо говорит «Рассуждаю» и показывает хвост мысли.
        assert!(line_text(&loader_line(&app)).contains("Рассуждаю"));
        let lines = loader_lines(&app, 80);
        assert!(
            lines
                .iter()
                .any(|l| line_text(l).contains("сверю с файлами")),
            "виден свежий кусок мысли"
        );
        // Как пошёл ответ — рассуждение убирается, фраза снова обычная.
        app.live_answer = "Ответ".to_string();
        assert!(!line_text(&loader_line(&app)).contains("Рассуждаю"));
        assert!(!loader_lines(&app, 80)
            .iter()
            .any(|l| line_text(l).contains("сверю с файлами")));
    }

    #[test]
    fn loader_shimmers_only_during_active_work() {
        let mut app = App::new();
        app.run_started_at = Some(Instant::now());
        // Тишина ожидания: фраза статична (фраза = один спан + деталь).
        let idle = loader_line(&app);
        // Пошёл ответ → активная работа → шиммер (спан на символ, заметно дробнее).
        app.live_answer = "ответ".to_string();
        let active = loader_line(&app);
        assert!(
            active.spans.len() > idle.spans.len(),
            "активная фраза переливается: idle={} active={}",
            idle.spans.len(),
            active.spans.len()
        );
        // Фраза отражает состояние, а не таймер.
        assert!(line_text(&active).contains("Пишу ответ"));
        assert!(line_text(&idle).contains("Думаю"));
    }

    #[test]
    fn loader_shimmer_uses_current_theme_palette() {
        assert_eq!(
            theme_shimmer_color(Theme::Amber, 1, 0),
            Theme::Amber.accent()
        );
        assert_ne!(
            theme_shimmer_color(Theme::Amber, 1, 0),
            Theme::Purple.accent()
        );
    }

    #[test]
    fn idle_loader_line_shows_verb_and_elapsed() {
        let app = App::new();
        let line = idle_loader_line(&app, Duration::from_secs(112));
        let text = line_text(&line);
        assert!(text.starts_with("✻ "), "inactive icon: {text}");
        assert!(text.contains("1m 52s"), "elapsed formatted: {text}");
        let ru = ["Думал", "Размышлял", "Соображал", "Кумекал", "Прикидывал"];
        assert!(ru.iter().any(|v| text.contains(v)), "verb from set: {text}");
    }

    #[test]
    fn idle_verb_is_deterministic_and_localized() {
        // Тот же seed → тот же глагол (не мигает между кадрами).
        assert_eq!(idle_verb(Language::Ru, 7), idle_verb(Language::Ru, 7));
        // Выбор реально варьируется по seed, а не залипает на одном слове.
        let v0 = idle_verb(Language::Ru, 0);
        assert!(
            (0..5u128).any(|s| idle_verb(Language::Ru, s) != v0),
            "разные seed дают разные глаголы"
        );
        // Наборы Ru и En не пересекаются.
        let en = ["Thought", "Pondered", "Cogitated", "Mused", "Reasoned"];
        let ru = ["Думал", "Размышлял", "Соображал", "Кумекал", "Прикидывал"];
        let en_verb = idle_verb(Language::En, 3);
        assert!(en.contains(&en_verb), "en verb из своего набора");
        assert!(!ru.contains(&en_verb), "en не из ru-набора");
    }
}
