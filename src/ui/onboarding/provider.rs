use super::*;

pub(crate) fn onboarding_provider_lines(app: &App, onboarding: &Onboarding) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::styled(
            app.lang
                .choose("Выбор связки моделей", "Choose model pairing"),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(onboarding.message.clone()),
        Line::from(""),
    ];

    for index in 0..provider_count() {
        let selected = index == onboarding.provider_index;
        let mode = provider_mode(index);
        let style = if selected {
            Style::default()
                .fg(Color::White)
                .bg(app.theme.accent_bg())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.accent_soft())
        };
        lines.push(Line::from(vec![
            Span::styled(
                if selected { "› " } else { "  " },
                Style::default().fg(app.theme.accent()),
            ),
            Span::styled(format!("{:<14}", mode.as_str()), style),
            Span::raw(" "),
            Span::styled(
                provider_description(mode, app.lang),
                Style::default().fg(MUTED),
            ),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::styled(
        app.lang.choose(
            "↑/↓ выбрать · Enter продолжить · Ctrl+C выйти",
            "↑/↓ choose · Enter continue · Ctrl+C exit",
        ),
        Style::default().fg(MUTED),
    ));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::onboarding::testkit::*;

    /// Список связок непустой и осмысленный, а маркер `›` стоит ровно на выбранной
    /// строке — и только на ней.
    #[test]
    fn provider_lines_list_providers_and_mark_the_selected_one() {
        let app = onboarding_app();
        let onb = Onboarding {
            provider_index: 1,
            ..onboarding(OnboardingStep::Provider)
        };
        let lines = onboarding_provider_lines(&app, &onb);

        // 46:5 `→ vec![]` и `→ vec![Default::default()]`: осмысленный непустой контент.
        let text = joined(&lines);
        assert!(text.contains("Choose model pairing"), "нет заголовка шага");
        assert!(text.contains("choose"), "нет подсказки навигации");
        assert!(
            text.contains("Claude drafts, Codex reviews"),
            "нет описания провайдера"
        );

        // 59:30 `== → !=`: провайдер-строки идут после 3 шапочных.
        let provider_lines = &lines[3..3 + provider_count()];
        let marked: Vec<usize> = provider_lines
            .iter()
            .enumerate()
            .filter(|(_, line)| span_texts(line)[0] == "› ")
            .map(|(index, _)| index)
            .collect();
        // Ровно одна помечена (а не три), и это именно provider_index.
        assert_eq!(marked, vec![1], "маркер обязан стоять на выбранной строке");
    }

    // ── Часть 3. onboarding_auth_lines ───────────────────────────────────────
}
