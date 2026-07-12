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
        _ => {}
    }
}
