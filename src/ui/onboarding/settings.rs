use super::*;

pub(crate) fn onboarding_settings_lines(app: &App, onboarding: &Onboarding) -> Vec<Line<'static>> {
    let rows = [
        (
            app.lang.choose("Раунды ревью", "Review rounds").to_string(),
            app.rounds.to_string(),
        ),
        ("Effort".to_string(), app.effort_summary()),
        (
            app.lang.choose("Язык", "Language").to_string(),
            app.lang.as_str().to_string(),
        ),
    ];

    let mut lines = vec![
        Line::styled(
            app.lang.choose("Стартовые настройки", "Startup settings"),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(onboarding.message.clone()),
        Line::from(""),
    ];

    for (index, (label, value)) in rows.into_iter().enumerate() {
        let selected = index == onboarding.setting_index;
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
            Span::styled(format!("{label:<18}"), style),
            Span::raw(" "),
            Span::styled(value, Style::default().fg(Color::White)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            app.lang.choose("Режим ", "Mode "),
            Style::default().fg(MUTED),
        ),
        Span::styled(
            app.mode.as_str(),
            Style::default()
                .fg(app.theme.accent_soft())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.lang.choose(" · Артефакты ", " · Artifacts "),
            Style::default().fg(MUTED),
        ),
        Span::styled(
            app.out_dir.clone(),
            Style::default().fg(app.theme.accent_soft()),
        ),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::styled(
        app.lang.choose(
            "↑/↓ поле · ←/→ изменить · L язык · Enter сохранить · Esc назад",
            "↑/↓ field · ←/→ change · L language · Enter save · Esc back",
        ),
        Style::default().fg(MUTED),
    ));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::onboarding::testkit::*;

    /// Экран настроек показывает все поля и метку режима, а маркер стоит ровно на
    /// выбранном поле.
    #[test]
    fn settings_lines_show_all_rows_and_mark_the_selected_one() {
        let app = onboarding_app();
        let onb = Onboarding {
            setting_index: 1,
            ..onboarding(OnboardingStep::Settings)
        };
        let lines = onboarding_settings_lines(&app, &onb);

        // 138:5 `→ vec![]` и `→ vec![Default::default()]`: осмысленный непустой контент.
        let text = joined(&lines);
        assert!(text.contains("Startup settings"), "нет заголовка");
        assert!(text.contains("Review rounds"), "нет поля раундов");
        assert!(text.contains("Effort"), "нет поля effort");
        assert!(text.contains("Language"), "нет поля языка");
        assert!(text.contains("Mode"), "нет метки режима");

        // 162:30 `== → !=`: строки настроек идут после 3 шапочных.
        let setting_lines = &lines[3..6];
        let marked: Vec<usize> = setting_lines
            .iter()
            .enumerate()
            .filter(|(_, line)| span_texts(line)[0] == "› ")
            .map(|(index, _)| index)
            .collect();
        // Ровно одна помечена (а не две), и это строка «Effort».
        assert_eq!(marked, vec![1], "маркер обязан стоять на выбранном поле");
    }

    // ── Часть 5. auth_status_line ────────────────────────────────────────────
}
