use crate::prelude::*;
use crate::*;

pub(crate) mod ask;
pub(crate) mod chats;
pub(crate) mod command_palette;
pub(crate) mod effort;
pub(crate) mod footer;
pub(crate) mod helpers;
pub(crate) mod loader;
pub(crate) mod onboarding;
pub(crate) mod plan_gate;
pub(crate) mod plugins;
pub(crate) mod prompt;
pub(crate) mod search;
pub(crate) mod settings;
pub(crate) mod shortcuts;
pub(crate) mod transcript;

pub(crate) use ask::*;
pub(crate) use chats::*;
pub(crate) use command_palette::*;
pub(crate) use effort::*;
pub(crate) use footer::*;
pub(crate) use helpers::*;
pub(crate) use loader::*;
pub(crate) use onboarding::*;
pub(crate) use plan_gate::*;
pub(crate) use plugins::*;
pub(crate) use prompt::*;
pub(crate) use search::*;
pub(crate) use settings::*;
pub(crate) use shortcuts::*;
pub(crate) use transcript::*;

// ── Живой нижний регион (фиксированная высота, перерисовка на месте) ─────────

/// Сколько строк показывает палитра команд (самая высокая панель).
pub(crate) const COMMAND_PALETTE_ROWS: u16 = 12;

/// Открыта ли под-композерная панель (палитра/подсказки/поиск/гейт). Когда открыта,
/// футер прячется: панель сама несёт контекст, дублировать подсказки незачем, да и
/// строку у панели отъедать не нужно. Условия должны совпадать с `panel_height`.
pub(crate) fn panel_active(app: &App) -> bool {
    app.ask_active()
        || normalized_command_query(&app.input).is_some()
        || app.overlay == Overlay::Shortcuts
        || app.overlay == Overlay::Search
        || app.plan_gate_active()
}

/// Высота под-футерной панели (палитра/подсказки/поиск/гейт), обрезанная по месту.
/// Loader сюда НЕ входит — он рисуется над вводом (в области диалога), не под футером.
pub(crate) fn panel_height(app: &App, width: u16, cap: u16) -> u16 {
    let height = if let Some(state) = &app.ask {
        return ask_panel_height(state, width, cap);
    } else if normalized_command_query(&app.input).is_some() {
        COMMAND_PALETTE_ROWS
    } else if app.overlay == Overlay::Shortcuts {
        shortcuts_panel_height(width)
    } else if app.overlay == Overlay::Search {
        search_panel_height()
    } else if app.plan_gate_active() {
        plan_gate_panel_height()
    } else {
        0
    };
    height.min(cap)
}

pub(crate) fn draw_active_panel(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.height == 0 {
        return;
    }
    if app.ask_active() {
        draw_ask_panel(frame, area, app);
    } else if normalized_command_query(&app.input).is_some() {
        draw_command_screen(frame, area, app);
    } else if app.overlay == Overlay::Shortcuts {
        draw_shortcuts_panel(frame, area, app);
    } else if app.overlay == Overlay::Search {
        draw_search_panel(frame, area, app);
    } else if app.plan_gate_active() {
        draw_plan_gate_panel(frame, area, app);
    }
}

/// Полноэкранная модалка (effort/settings/chats/onboarding) во временном alt-screen.
pub(crate) fn draw_modal(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    if app.onboarding.is_some() {
        draw_onboarding(frame, area, app);
        return;
    }
    match app.overlay {
        Overlay::Effort => draw_effort_screen(frame, area, app),
        Overlay::Settings => draw_settings_screen(frame, area, app),
        Overlay::Chats => draw_chats_screen(frame, area, app),
        Overlay::Plugins => draw_plugins_screen(frame, area, app),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, buffer::Buffer};

    fn mod_app() -> App {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = std::env::temp_dir().join(format!(
            "clave-uimod-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::create_dir_all(&dir);
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
        app.lang = Language::En;
        app
    }

    fn buffer_rows(buffer: &Buffer) -> Vec<String> {
        let area = buffer.area;
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .filter_map(|x| buffer.cell((x, y)))
                    .map(|cell| cell.symbol())
                    .collect::<String>()
            })
            .collect()
    }

    fn active_panel_rows(app: &App, w: u16, h: u16) -> Vec<String> {
        let mut term = Terminal::new(TestBackend::new(w, h)).expect("term");
        term.draw(|f| draw_active_panel(f, f.area(), app))
            .expect("draw");
        let buf = term.backend().buffer().clone();
        buffer_rows(&buf)
    }

    fn modal_rows(app: &App, w: u16, h: u16) -> Vec<String> {
        let mut term = Terminal::new(TestBackend::new(w, h)).expect("term");
        term.draw(|f| draw_modal(f, app)).expect("draw");
        let buf = term.backend().buffer().clone();
        buffer_rows(&buf)
    }

    fn is_non_empty(rows: &[String]) -> bool {
        rows.iter().any(|row| row.chars().any(|c| c != ' '))
    }

    // ── Часть 1. panel_active: ровно одно активное условие ловит и →false, и каждый ||→&& ──

    /// Открытая палитра команд поднимает панель. Ловит 43:5 (→false), а также ||→&&
    /// на слагаемых, где команда стоит по краю ИЛИ середине конъюнкции: ask=false и
    /// overlay=None гасят соседей, так что 44:9 и 45:9 роняют true→false.
    #[test]
    fn panel_is_active_when_a_command_query_is_open() {
        let mut app = mod_app();
        app.input = "/help".to_string();
        assert!(
            panel_active(&app),
            "открытая команда обязана поднять панель"
        );
    }

    /// Оверлей подсказок поднимает панель в одиночку — ловит ||→&& на своём слагаемом (46:9).
    #[test]
    fn panel_is_active_for_the_shortcuts_overlay() {
        let mut app = mod_app();
        app.overlay = Overlay::Shortcuts;
        assert!(panel_active(&app));
    }

    /// Оверлей поиска — то же для последнего слагаемого (47:9).
    #[test]
    fn panel_is_active_for_the_search_overlay() {
        let mut app = mod_app();
        app.overlay = Overlay::Search;
        assert!(panel_active(&app));
    }

    /// Пустой App: ни одно условие не выполнено — панели нет. Контраст к 43:5.
    #[test]
    fn panel_is_inactive_in_a_fresh_app() {
        let app = mod_app();
        assert!(!panel_active(&app));
    }

    // ── Часть 2. panel_height: точная высота, не «>0» ─────────────────────────

    /// При открытой палитре высота — ровно COMMAND_PALETTE_ROWS (12). Мутант 53:5 (→0)
    /// вернул бы ноль, поэтому сверяем точным числом.
    #[test]
    fn panel_height_for_a_command_query_is_the_palette_height() {
        let mut app = mod_app();
        app.input = "/help".to_string();
        assert_eq!(panel_height(&app, 80, 20), COMMAND_PALETTE_ROWS);
        assert_eq!(COMMAND_PALETTE_ROWS, 12, "палитра — 12 строк");
    }

    // ── Часть 3. draw_active_panel: непустой кадр нужной панели ────────────────

    /// Палитра команд рисуется. Ловит 70:5 (→()) и 70:20 (==→!=): при height!=0 мутант
    /// сразу выходит и оставляет пустой кадр.
    #[test]
    fn active_panel_draws_the_command_palette() {
        let mut app = mod_app();
        app.input = "/help".to_string();
        assert!(is_non_empty(&active_panel_rows(&app, 80, 12)));
    }

    /// Панель подсказок рисуется. Ловит 77:27 (==→!=): инвертированное условие пропустит ветку.
    #[test]
    fn active_panel_draws_the_shortcuts_panel() {
        let mut app = mod_app();
        app.overlay = Overlay::Shortcuts;
        assert!(is_non_empty(&active_panel_rows(&app, 80, 12)));
    }

    /// Панель поиска рисуется. Ловит 79:27 (==→!=).
    #[test]
    fn active_panel_draws_the_search_panel() {
        let mut app = mod_app();
        app.overlay = Overlay::Search;
        assert!(is_non_empty(&active_panel_rows(&app, 80, 12)));
    }

    /// Пустой App: рисовать нечего — кадр пуст. Контраст, отсекающий «панель рисует всегда».
    #[test]
    fn active_panel_is_empty_without_an_active_panel() {
        let app = mod_app();
        assert!(!is_non_empty(&active_panel_rows(&app, 80, 12)));
    }

    // ── Часть 4. draw_modal: непустой кадр + стабильный заголовок ──────────────

    /// Модалка effort рисуется. Ловит 88:5 (→()) и 95:9 (удаление плеча Effort → только Clear).
    #[test]
    fn modal_draws_the_effort_screen() {
        let mut app = mod_app();
        app.overlay = Overlay::Effort;
        let rows = modal_rows(&app, 80, 24);
        assert!(is_non_empty(&rows));
        assert!(
            rows.iter().any(|r| r.contains("/effort")),
            "нет заголовка effort"
        );
    }

    /// Модалка settings рисуется. Ловит 96:9 (удаление плеча Settings).
    #[test]
    fn modal_draws_the_settings_screen() {
        let mut app = mod_app();
        app.overlay = Overlay::Settings;
        let rows = modal_rows(&app, 80, 24);
        assert!(is_non_empty(&rows));
        assert!(
            rows.iter().any(|r| r.contains("/settings")),
            "нет заголовка settings"
        );
    }

    /// Модалка chats рисуется. Ловит 97:9 (удаление плеча Chats).
    #[test]
    fn modal_draws_the_chats_screen() {
        let mut app = mod_app();
        app.overlay = Overlay::Chats;
        let rows = modal_rows(&app, 80, 24);
        assert!(is_non_empty(&rows));
        assert!(
            rows.iter().any(|r| r.contains("/resume")),
            "нет заголовка chats"
        );
    }
}
