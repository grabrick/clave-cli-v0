use super::*;

mod layout;
mod slot;
pub(crate) use layout::*;
pub(crate) use slot::*;

pub(crate) fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.height == 0 {
        return;
    }

    let git = footer_git_segment(app);

    if let Some((message, shown_at)) = &app.footer_notice {
        if shown_at.elapsed() <= Duration::from_secs(2) {
            draw_footer_notice(frame, area, app, message, git.as_deref());
            return;
        }
    }

    let mode_label = app.chat_mode.label(app.lang);
    let switch = MODE_SWITCH_KEYS;
    let hints = app
        .lang
        .choose("? подсказки · / команды", "? shortcuts · / commands");
    let (right, right_style) = footer_right_segment(app);

    let layout = footer_layout(
        area.width as usize,
        mode_label,
        switch,
        hints,
        git.as_deref(),
        &right,
        footer_right_slot_width(app),
    );

    // Разделитель перед слотом рисуем, только когда индикатор показан: его ширина уже
    // заложена в бюджет раскладки (GIT_GAP), без спана git прилип бы к правому сегменту.
    let git_gap = if layout.git.is_empty() { 0 } else { GIT_GAP };
    let line = Line::from(vec![
        Span::styled(
            mode_label,
            Style::default()
                .fg(app.chat_mode.color())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(switch, Style::default().fg(MUTED)),
        Span::raw("  "),
        Span::styled(layout.hints, Style::default().fg(app.theme.accent_soft())),
        Span::raw(" ".repeat(layout.gap)),
        Span::styled(layout.git, Style::default().fg(MUTED)),
        Span::raw(" ".repeat(git_gap + layout.right_padding)),
        Span::styled(layout.right, right_style),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

/// Уведомление занимает футер целиком на 2 секунды — но индикатор постоянный, поэтому
/// он остаётся у правого края и здесь (если помещается рядом с текстом уведомления).
fn draw_footer_notice(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    message: &str,
    git: Option<&str>,
) {
    let budget = (area.width as usize).saturating_sub(2);
    let notice_style = Style::default()
        .fg(app.theme.accent_soft())
        .add_modifier(Modifier::BOLD);

    let git_total = git
        .map(|git| display_width(git) + GIT_GAP)
        .filter(|total| budget > *total)
        .unwrap_or(0);
    if git_total == 0 {
        let text = truncate_display(message, area.width as usize);
        frame.render_widget(Paragraph::new(text).style(notice_style), area);
        return;
    }

    let message = truncate_display(message, budget - git_total);
    let gap = budget.saturating_sub(display_width(&message) + git_total);
    let line = Line::from(vec![
        Span::styled(message, notice_style),
        Span::raw(" ".repeat(gap + GIT_GAP)),
        Span::styled(
            git.unwrap_or_default().to_string(),
            Style::default().fg(MUTED),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;

    /// Изолированный App на временных путях: `App::new()` читал бы настоящий конфиг ~/.clave
    /// и будил бы auth-пробы — тесты стали бы флейкими и зависели бы от машины.
    pub(crate) fn bare_app() -> App {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "clave-uifooter-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::create_dir_all(&dir);
        App::from_config(
            AppConfig {
                onboarding_done: true,
                ..AppConfig::default()
            },
            dir.join("config.json"),
            dir.join("history"),
            dir.clone(),
        )
    }
}
