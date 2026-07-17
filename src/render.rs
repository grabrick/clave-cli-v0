use crate::prelude::*;
use crate::*;

use crossterm::{
    cursor::{Hide, MoveDown, MoveRight, MoveTo, MoveToColumn, MoveUp, Show},
    queue,
    style::{
        Attribute as CtAttr, Color as CtColor, Print, ResetColor, SetAttribute, SetBackgroundColor,
        SetForegroundColor,
    },
    terminal::{Clear, ClearType, SetTitle},
};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

/// Живой нижний блок, перерисовываемый «на месте» (модель Ink / Claude Code).
///
/// История уходит в НАТИВНЫЙ скроллбэк терминала (печатается один раз), а блок
/// `[панель|loader][поле ввода][футер]` каждый кадр стирается и рисуется заново
/// прямо под историей. Высота блока меняется свободно — поэтому открытие меню
/// разворачивает блок «на месте» без сдвига истории и без накопления пустоты, а
/// закрытие чисто его схлопывает. Колесо/выделение работают (история = скроллбэк).
pub(crate) struct LiveRenderer {
    started: bool,
    /// Высота блока в прошлом кадре (строк на экране).
    prev_height: u16,
    /// На сколько строк выше нижней строки блока стоял курсор ввода.
    cursor_above: u16,
    /// Строки блока прошлого кадра — для дифф-перерисовки (правим только изменившиеся).
    prev_lines: Vec<Line<'static>>,
    /// Позиция курсора ввода прошлого кадра (строка, столбец) внутри блока.
    prev_cursor: (u16, u16),
    /// Последний выставленный title окна терминала.
    prev_terminal_title: String,
}

impl LiveRenderer {
    pub(crate) fn new() -> Self {
        Self {
            started: false,
            prev_height: 0,
            cursor_above: 0,
            prev_lines: Vec::new(),
            prev_cursor: (0, 0),
            prev_terminal_title: String::new(),
        }
    }

    /// Заставляет следующий кадр перерисоваться полностью (после модалок/внешних команд).
    pub(crate) fn invalidate(&mut self) {
        self.prev_lines.clear();
    }

    /// Кадр: вытесняет новую историю в скроллбэк и обновляет живой блок.
    ///
    /// Полная перерисовка блока только при структурных изменениях (новая история,
    /// смена высоты, первый кадр). В остальных случаях — ДИФФ по строкам: правим
    /// лишь изменившиеся (цвет/текст), не трогая остальные → нет мерцания футера, а
    /// анимация появления палитры (меняется цвет) проигрывается.
    pub(crate) fn render(&mut self, app: &mut App, width: u16, full_h: u16) -> io::Result<()> {
        let mut out = io::stdout().lock();
        self.render_to(&mut out, app, width, full_h)
    }

    /// То же, но в ЛЮБОЙ приёмник.
    ///
    /// Шов ради тестов. Рендерер писал прямо в `io::stdout()`, и проверить его было нечем:
    /// мутационный прогон показал 46 выживших мутантов в одном этом методе — то есть живой блок
    /// мог дублироваться, курсор уезжать, история печататься дважды, и НИ ОДИН тест этого бы не
    /// заметил. Единственным способом убедиться, что экран не рассыпался, оставались глаза.
    ///
    /// Приём не новый: `queue_line` и `queue_rich_line` в этом же файле давно берут приёмник
    /// параметром. Тут замысел просто доведён до конца.
    pub(crate) fn render_to(
        &mut self,
        out: &mut impl Write,
        app: &mut App,
        width: u16,
        full_h: u16,
    ) -> io::Result<()> {
        self.sync_terminal_title_to(out, app)?;

        // Полная очистка терминала по запросу (/clear, /new, /resume): стираем
        // экран И нативный скроллбэк, иначе старая напечатанная история остаётся.
        if app.pending_clear_screen {
            app.pending_clear_screen = false;
            self.wipe_screen(out)?;
        }

        // Полная перерисовка после ресайза: терминал перелил историю под новую
        // ширину, а наш кэш позиций (prev_height/cursor_above) описывает старую
        // геометрию — относительные сдвиги курсора «съедут» и живой блок начнёт
        // дублироваться. Чистим экран И скроллбэк, сбрасываем счётчик истории и
        // состояние подсветки, чтобы структурный путь ниже перепечатал всё заново.
        if app.pending_full_redraw {
            app.pending_full_redraw = false;
            self.wipe_screen(out)?;
            app.scrollback_count = 0;
            app.flush_state = TranscriptRenderState::default();
        }

        let (lines, cur_row, cur_col) = build_dynamic(app, width, full_h);
        let has_new_history = app.scrollback_count < app.transcript.len();
        let structural = !self.started || has_new_history || lines.len() != self.prev_lines.len();

        if !structural && lines == self.prev_lines && (cur_row, cur_col) == self.prev_cursor {
            return Ok(()); // ничего не изменилось
        }

        let height = lines.len() as u16;
        let last = height.saturating_sub(1);
        queue!(out, Hide)?;

        if structural {
            // Полная перерисовка: стереть старый блок, вывести новую историю, блок.
            if self.started {
                if self.cursor_above > 0 {
                    queue!(out, MoveDown(self.cursor_above))?;
                }
                queue!(out, MoveToColumn(0))?;
                if self.prev_height > 1 {
                    queue!(out, MoveUp(self.prev_height - 1))?;
                }
            } else {
                queue!(out, MoveToColumn(0))?;
            }
            queue!(out, Clear(ClearType::FromCursorDown))?;

            let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            while app.scrollback_count < app.transcript.len() {
                let raw = app.transcript[app.scrollback_count].clone();
                let rows = history_rich_render(
                    &raw,
                    app.lang,
                    width,
                    app.theme,
                    &mut app.flush_state,
                    app.path_link_target,
                    &cwd,
                );
                for row in &rows {
                    queue_rich_line(out, row)?;
                    queue!(out, Clear(ClearType::UntilNewLine), Print("\r\n"))?;
                }
                app.scrollback_count += 1;
            }

            for (index, line) in lines.iter().enumerate() {
                queue_line(out, line)?;
                queue!(out, Clear(ClearType::UntilNewLine))?;
                if index + 1 < lines.len() {
                    queue!(out, Print("\r\n"))?;
                }
            }
        } else {
            // Дифф: встать на верх блока и перерисовать только изменившиеся строки.
            if self.cursor_above > 0 {
                queue!(out, MoveDown(self.cursor_above))?;
            }
            queue!(out, MoveToColumn(0))?;
            if last > 0 {
                queue!(out, MoveUp(last))?;
            }
            for (index, line) in lines.iter().enumerate() {
                queue!(out, MoveToColumn(0))?;
                if self.prev_lines.get(index) != Some(line) {
                    queue_line(out, line)?;
                    queue!(out, Clear(ClearType::UntilNewLine))?;
                }
                if index + 1 < lines.len() {
                    queue!(out, MoveDown(1))?;
                }
            }
        }

        // Поставить курсор в поле ввода (он сейчас на последней строке блока).
        queue!(out, MoveToColumn(0))?;
        if last > cur_row {
            queue!(out, MoveUp(last - cur_row))?;
        }
        if cur_col > 0 {
            queue!(out, MoveRight(cur_col))?;
        }
        queue!(out, Show)?;
        out.flush()?;

        self.prev_height = height;
        self.cursor_above = last.saturating_sub(cur_row);
        self.prev_lines = lines;
        self.prev_cursor = (cur_row, cur_col);
        self.started = true;
        Ok(())
    }

    /// Стереть экран И нативный скроллбэк, сбросив кэш позиций живого блока.
    ///
    /// Один код на два повода (`/clear` и ресайз): раньше он был скопирован дважды, и правка в
    /// одном месте молча расходилась бы со вторым.
    fn wipe_screen(&mut self, out: &mut impl Write) -> io::Result<()> {
        queue!(
            out,
            Clear(ClearType::All),
            Clear(ClearType::Purge),
            MoveTo(0, 0)
        )?;
        out.flush()?;
        self.started = false;
        self.prev_height = 0;
        self.cursor_above = 0;
        self.prev_lines.clear();
        Ok(())
    }

    fn sync_terminal_title_to(&mut self, out: &mut impl Write, app: &App) -> io::Result<()> {
        let title = terminal_window_title(app);
        if title == self.prev_terminal_title {
            return Ok(());
        }
        execute!(out, SetTitle(&title))?;
        self.prev_terminal_title = title;
        Ok(())
    }

    /// Перед внешней командой: СТИРАЕТ живой блок целиком, оставляя на экране
    /// историю диалога. Вывод команды печатается на месте блока, а блок потом
    /// перерисуется (invalidate). Для выхода из приложения см. `clear_for_exit`.
    pub(crate) fn leave_below(&mut self) -> io::Result<()> {
        let mut out = io::stdout().lock();
        self.leave_below_to(&mut out)
    }

    pub(crate) fn leave_below_to(&mut self, out: &mut impl Write) -> io::Result<()> {
        if !self.started {
            return Ok(());
        }
        // встать на нижнюю строку блока → на верх блока → стереть от курсора вниз
        if self.cursor_above > 0 {
            queue!(out, MoveDown(self.cursor_above))?;
        }
        queue!(out, MoveToColumn(0))?;
        if self.prev_height > 1 {
            queue!(out, MoveUp(self.prev_height - 1))?;
        }
        queue!(out, Clear(ClearType::FromCursorDown), Show)?;
        out.flush()?;
        self.started = false;
        self.prev_height = 0;
        self.cursor_above = 0;
        self.prev_lines.clear();
        Ok(())
    }

    /// При выходе из приложения: ЧИСТЫЙ выход — стираем экран И нативный скроллбэк,
    /// чтобы беседа не оставалась в терминале после закрытия (как `/clear`). Сама
    /// беседа сохранена в файле чата — вернуть можно через `/chats`.
    pub(crate) fn clear_for_exit(&mut self, app: &App) -> io::Result<()> {
        let mut out = io::stdout().lock();
        self.clear_for_exit_to(&mut out, app)
    }

    pub(crate) fn clear_for_exit_to(&mut self, out: &mut impl Write, _app: &App) -> io::Result<()> {
        queue!(
            out,
            MoveToColumn(0),
            Clear(ClearType::All),
            Clear(ClearType::Purge),
            MoveTo(0, 0),
            Show
        )?;
        out.flush()?;
        self.started = false;
        self.prev_height = 0;
        self.cursor_above = 0;
        self.prev_lines.clear();
        Ok(())
    }
}

pub(crate) fn terminal_window_title(app: &App) -> String {
    terminal_title_for(app.chat_title_custom, &app.chat_title)
}

/// Заголовок, который СТАВИТ clave: только имя ЯВНО названного чата (/name, /rename).
/// Путь, имя процесса и размер терминал показывает сам — clave их НЕ дублирует (иначе
/// получалось "Macintosh HD — / — clave — clave — 133x24"). Безымянный чат → пустая
/// строка: терминал рисует свой дефолт, clave заголовок не трогает.
pub(crate) fn terminal_title_for(custom: bool, chat_title: &str) -> String {
    if custom && !chat_title.trim().is_empty() {
        sanitize_terminal_title_fragment(chat_title)
    } else {
        String::new()
    }
}

fn sanitize_terminal_title_fragment(text: &str) -> String {
    let cleaned = text
        .chars()
        .filter(|ch| !ch.is_control())
        .collect::<String>();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        APP_COMMAND.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Рендерит живой блок в оффскрин-буфер (переиспользуя обычные виджеты ratatui,
/// включая рамки) и возвращает его строки + позицию курсора ввода в блоке.
fn build_dynamic(app: &App, width: u16, full_h: u16) -> (Vec<Line<'static>>, u16, u16) {
    let width = width.max(1);
    let composer = composer_height(app, width);
    // Футер прячется, когда открыта панель (палитра/подсказки/поиск/гейт): она сама
    // под композером, дублировать подсказки и отъедать строку незачем.
    let footer = if panel_active(app) { 0 } else { 1 };
    // «Воздух» только сверху блока: пустая строка между историей и блоком (работает и
    // под лоадером — он не липнет к тексту). Под инпутом отступ не нужен — футер идёт
    // сразу за нижней линейкой композера.
    let gap_top = 1u16;
    let reserved = gap_top + composer + footer;
    let room = full_h
        .saturating_sub(1) // оставить хотя бы строку под историю/скроллбэк
        .saturating_sub(reserved);
    // Верхний слот над вводом (область диалога): реплика пользователя текущего рана
    // (live_turn, ещё не в ленте) сверху, под ней «печать» ответа (reveal) или loader.
    let mut top: Vec<Line<'static>> = Vec::new();
    if let Some(turn) = &app.live_turn {
        let mut state = TranscriptRenderState::default();
        let mut turn_lines = history_line_render(turn, app.lang, width, app.theme, &mut state);
        // ведущую пустую строку из бокса убираем — воздух уже даёт gap_top
        if turn_lines.first().is_some_and(|line| line.width() == 0) {
            turn_lines.remove(0);
        }
        top.extend(turn_lines);
    }
    if let Some(reveal) = &app.reveal {
        let shown = reveal.shown_text();
        let mut state = TranscriptRenderState::default();
        top.extend(
            shown
                .split('\n')
                .flat_map(|line| history_line_render(line, app.lang, width, app.theme, &mut state)),
        );
    } else if app.running {
        // Живой токен-стрим ответа (claude): растёт по мере прихода, рисуется как
        // обычный ответ (⏺); лоадер со спиннером/активностью — под ним.
        // Прячем тело блока ```clave-ask` ещё в стриме: JSON выбора не должен
        // мелькать в ленте до того, как откроется панель (на ChatDone).
        let visible = live_answer_visible(&app.live_answer);
        if !visible.is_empty() {
            let shown = format!("⏺ {visible}");
            let mut state = TranscriptRenderState::default();
            top.extend(shown.split('\n').flat_map(|line| {
                history_line_render(line, app.lang, width, app.theme, &mut state)
            }));
        }
        // Отступ перед лоадером, когда сверху уже есть контент (реплика live_turn
        // или печатаемый ответ) — иначе спиннер липнет к тексту. Если контента нет,
        // верхнюю пустую строку уже даёт gap_top, второй отступ был бы двойным.
        if !top.is_empty() {
            top.push(Line::from(""));
        }
        top.extend(loader_lines(app, width));
        // Воздух между лоадером и полем ввода: спиннер не липнет к инпуту.
        // Пустая строка — последняя в top, окно всегда держит хвост → она ровно
        // над композером (правка раскладки/курсора не нужна).
        top.push(Line::from(""));
    } else if let Some(d) = app.last_run_duration {
        // Ран завершён: «замороженный» лоадер остаётся над инпутом до следующего
        // ввода. Верхний воздух даёт gap_top, снизу — пустая строка над композером.
        top.push(idle_loader_line(app, d));
        top.push(Line::from(""));
    }
    let top_h = (top.len() as u16).min(room);
    // Если reveal длиннее окна — показываем хвост (низ), как стрим в терминале.
    let top_tail: Vec<Line<'static>> = top.split_off(top.len() - top_h as usize);
    let panel = panel_height(app, width, room.saturating_sub(top_h));
    let height = (gap_top + top_h + composer + footer + panel)
        .min(full_h.saturating_sub(1).max(1))
        .max(composer + footer);

    let mut terminal = match Terminal::new(TestBackend::new(width, height)) {
        Ok(terminal) => terminal,
        Err(_) => return (Vec::new(), 0, 0),
    };
    // Порядок сверху вниз: воздух → reveal|loader → поле ввода → футер → панель.
    let lines = terminal
        .draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(gap_top),
                    Constraint::Length(top_h),
                    Constraint::Length(composer),
                    Constraint::Length(footer),
                    Constraint::Length(panel),
                ])
                .split(area);
            if top_h > 0 {
                frame.render_widget(Paragraph::new(top_tail), chunks[1]);
            }
            draw_prompt_bar(frame, chunks[2], app);
            if footer > 0 {
                draw_footer(frame, chunks[3], app);
            }
            if panel > 0 {
                draw_active_panel(frame, chunks[4], app);
            }
        })
        .map(|completed| buffer_to_lines(completed.buffer))
        .unwrap_or_default();

    // Курсор ввода: композер идёт после воздуха и верхнего слота, +1 на верхнюю
    // линейку композера (плашка названия встроена в неё, отдельной строки нет).
    let (line_index, col) = input_cursor_position_wrapped(&app.input, app.cursor, width);
    let cur_row = (gap_top + top_h + 1 + line_index as u16).min(height.saturating_sub(1));
    let cur_col = (2 + col as u16).min(width.saturating_sub(1));
    (lines, cur_row, cur_col)
}

/// Превращает строки оффскрин-буфера в `Line`, схлопывая одинаковые стили в спаны.
fn buffer_to_lines(buf: &Buffer) -> Vec<Line<'static>> {
    let area = buf.area;
    (0..area.height)
        .map(|y| {
            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut text = String::new();
            let mut current: Option<Style> = None;
            for x in 0..area.width {
                let Some(cell) = buf.cell((area.x + x, area.y + y)) else {
                    continue;
                };
                let style = Style::default()
                    .fg(cell.fg)
                    .bg(cell.bg)
                    .add_modifier(cell.modifier);
                if current != Some(style) {
                    if !text.is_empty() {
                        spans.push(Span::styled(
                            std::mem::take(&mut text),
                            current.unwrap_or_default(),
                        ));
                    }
                    current = Some(style);
                }
                text.push_str(cell.symbol());
            }
            if !text.is_empty() {
                spans.push(Span::styled(text, current.unwrap_or_default()));
            }
            Line::from(spans)
        })
        .collect()
}

/// Убирает управляющие символы (ESC/CR/BEL/BS/…) из текста перед выводом в
/// терминал. Иначе ответ модели или содержимое прочитанного агентом файла могло бы
/// инжектить ANSI/OSC-последовательности (смена заголовка, OSC 52 → буфер обмена,
/// подмена UI). Цвет/стиль идут отдельно (`apply_style`), а не из контента, так что
/// собственный UI не страдает. Табы сохраняем; рамки/кириллица — не control, целы.
fn sanitize_terminal_text(text: &str) -> std::borrow::Cow<'_, str> {
    if text.chars().any(|ch| ch.is_control() && ch != '\t') {
        std::borrow::Cow::Owned(
            text.chars()
                .filter(|ch| !ch.is_control() || *ch == '\t')
                .collect(),
        )
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

fn queue_line(out: &mut impl Write, line: &Line<'static>) -> io::Result<()> {
    for span in &line.spans {
        apply_style(out, span.style)?;
        queue!(out, Print(sanitize_terminal_text(&span.content)))?;
        queue!(out, SetAttribute(CtAttr::Reset), ResetColor)?;
    }
    Ok(())
}

/// Как `queue_line`, но спаны со ссылкой оборачивает в OSC 8-гиперссылку. URL —
/// доверенный (строит `open_url`), а текст спана всё так же проходит через
/// `sanitize_terminal_text`: контентные ESC вырезаются, инъекция невозможна.
fn queue_rich_line(out: &mut impl Write, rich: &RichLine) -> io::Result<()> {
    for (index, span) in rich.line.spans.iter().enumerate() {
        let url = rich
            .links
            .iter()
            .find(|link| link.span == index)
            .map(|link| link.url.as_str());
        apply_style(out, span.style)?;
        if let Some(url) = url {
            queue!(out, Print(format!("\x1b]8;;{url}\x1b\\")))?;
        }
        queue!(out, Print(sanitize_terminal_text(&span.content)))?;
        if url.is_some() {
            queue!(out, Print("\x1b]8;;\x1b\\"))?;
        }
        queue!(out, SetAttribute(CtAttr::Reset), ResetColor)?;
    }
    Ok(())
}

fn apply_style(out: &mut impl Write, style: Style) -> io::Result<()> {
    if let Some(fg) = style.fg {
        queue!(out, SetForegroundColor(to_crossterm_color(fg)))?;
    }
    if let Some(bg) = style.bg {
        queue!(out, SetBackgroundColor(to_crossterm_color(bg)))?;
    }
    let modifier = style.add_modifier;
    if modifier.contains(Modifier::BOLD) {
        queue!(out, SetAttribute(CtAttr::Bold))?;
    }
    if modifier.contains(Modifier::DIM) {
        queue!(out, SetAttribute(CtAttr::Dim))?;
    }
    if modifier.contains(Modifier::ITALIC) {
        queue!(out, SetAttribute(CtAttr::Italic))?;
    }
    if modifier.contains(Modifier::UNDERLINED) {
        queue!(out, SetAttribute(CtAttr::Underlined))?;
    }
    if modifier.contains(Modifier::REVERSED) {
        queue!(out, SetAttribute(CtAttr::Reverse))?;
    }
    if modifier.contains(Modifier::CROSSED_OUT) {
        queue!(out, SetAttribute(CtAttr::CrossedOut))?;
    }
    Ok(())
}

/// Точное соответствие маппингу ratatui-crossterm (чтобы цвета совпадали 1:1).
fn to_crossterm_color(color: Color) -> CtColor {
    match color {
        Color::Reset => CtColor::Reset,
        Color::Black => CtColor::Black,
        Color::Red => CtColor::DarkRed,
        Color::Green => CtColor::DarkGreen,
        Color::Yellow => CtColor::DarkYellow,
        Color::Blue => CtColor::DarkBlue,
        Color::Magenta => CtColor::DarkMagenta,
        Color::Cyan => CtColor::DarkCyan,
        Color::Gray => CtColor::Grey,
        Color::DarkGray => CtColor::DarkGrey,
        Color::LightRed => CtColor::Red,
        Color::LightGreen => CtColor::Green,
        Color::LightBlue => CtColor::Blue,
        Color::LightYellow => CtColor::Yellow,
        Color::LightMagenta => CtColor::Magenta,
        Color::LightCyan => CtColor::Cyan,
        Color::White => CtColor::White,
        Color::Indexed(i) => CtColor::AnsiValue(i),
        Color::Rgb(r, g, b) => CtColor::Rgb { r, g, b },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Каталог уникален на процесс И на вызов: параллельные прогоны иначе затирают
    /// файлы друг друга, и мутационный гейт получает случайные падения.
    fn temp_render_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = std::env::temp_dir().join(format!(
            "clave-render-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    /// App на своих временных путях. Через `App::new()` нельзя: она читает настоящий
    /// конфиг пользователя и при непройденном онбординге поднимает auth-probe процессы.
    fn render_app() -> App {
        let dir = temp_render_dir();
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
        app.git_ref_detector = |_| None;
        app.transcript.clear();
        app
    }

    /// Кадр в память: ровно те байты, что ушли бы в терминал.
    fn frame(renderer: &mut LiveRenderer, app: &mut App, width: u16, full_h: u16) -> String {
        let mut out: Vec<u8> = Vec::new();
        renderer
            .render_to(&mut out, app, width, full_h)
            .expect("рендер в память не падает");
        String::from_utf8_lossy(&out).into_owned()
    }

    fn vt_visible(parser: &vt100::Parser) -> String {
        parser.screen().contents()
    }

    // Инвариант inline-рендера через терминал-эмулятор (vt100): инкрементальная отрисовка
    // серии кадров ОБЯЗАНА совпасть с чистой перерисовкой того же состояния с нуля. Любой
    // мусор от промаха MoveUp/Clear (недостёртый дубль или стёртое лишнее — обкатка BUG-001/003)
    // даёт расхождение. Гоняем у нижнего края маленького терминала и с ШИРОКИМИ (CJK) символами
    // в контенте — так проверяется, что высота блока считается по реальной ширине ячеек, а не по
    // числу символов. Оффскрин-буфер frame() такое показать не мог — оттого баг и уцелел в тестах.
    #[test]
    fn inline_render_matches_clean_redraw_across_stream() {
        let (cols, rows) = (72u16, 10u16);
        let mut parser = vt100::Parser::new(rows, cols, 1000);
        let mut r = LiveRenderer::new();
        let mut app = render_app();

        macro_rules! tick {
            () => {{
                let mut out: Vec<u8> = Vec::new();
                r.render_to(&mut out, &mut app, cols, rows).unwrap();
                parser.process(&out);
            }};
        }

        // Три хода чата: реплика → reasoning-стрим → ответ-стрим → завершение. Широкие CJK-
        // символы (中文) clave и терминал одинаково считают по 2 клетки — проверяем, что высота
        // и вертикальное позиционирование опираются на ячейки.
        for turn in 0..3 {
            app.live_turn = Some(format!("метка{turn} 中文 проверь контекст"));
            app.running = true;
            app.live_answer.clear();
            app.live_reasoning.clear();
            tick!();

            for step in 0..6 {
                app.live_reasoning
                    .push_str(&format!("думаю шаг {turn}.{step}\n"));
                tick!();
            }
            app.live_reasoning.clear();
            for line in 0..8 {
                app.live_answer
                    .push_str(&format!("ответ {turn}.{line} 中文 строка\n"));
                tick!();
            }

            if let Some(t) = app.live_turn.take() {
                app.push_system(t);
            }
            let answer_lines: Vec<String> = app
                .live_answer
                .split('\n')
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            for line in answer_lines {
                app.push_system(line);
            }
            app.live_answer.clear();
            app.running = false;
            tick!();
        }

        let incremental = vt_visible(&parser);

        // Та же лента, нарисованная с нуля новым рендерером во второй эмулятор.
        app.scrollback_count = 0;
        app.flush_state = TranscriptRenderState::default();
        let mut clean_parser = vt100::Parser::new(rows, cols, 1000);
        let mut clean = LiveRenderer::new();
        let mut out: Vec<u8> = Vec::new();
        clean.render_to(&mut out, &mut app, cols, rows).unwrap();
        clean_parser.process(&out);
        let from_scratch = vt_visible(&clean_parser);

        assert_eq!(
            incremental, from_scratch,
            "инкрементальный рендер разошёлся с чистой перерисовкой (промах MoveUp/Clear)\n\
             === инкрементально ===\n{incremental}\n=== с нуля ===\n{from_scratch}\n==="
        );
    }

    /// Приложение с ПОЛНОСТЬЮ детерминированным футером: правый слот берётся из полей
    /// (а не из стенных часов), панели и верхний слот погашены — значит футер это
    /// последняя строка блока, и её можно сверять посимвольно.
    fn footer_app() -> App {
        let mut app = render_app();
        app.onboarding = None;
        app.overlay = Overlay::None;
        app.ask = None;
        app.input.clear();
        app.cursor = 0;
        app.running = false;
        app.live_turn = None;
        app.reveal = None;
        app.last_run_duration = None;
        app.footer_notice = None;
        app.lang = Language::Ru;
        app.theme = Theme::Purple;
        app.chat_mode = ChatMode::Discussion;
        app.git_ref = Some("main".to_string());
        app.footer_right_text = "чат: Claude".to_string();
        app.footer_right_previous_text = None;
        app.footer_right_changed_at = None;
        app
    }

    /// Строка футера из оффскрин-буфера — ровно те символы, что увидит терминал.
    fn footer_line(app: &App, width: u16) -> String {
        let (lines, _, _) = build_dynamic(app, width, 20);
        lines
            .last()
            .expect("блок не пуст")
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    /// Слева направо: режим, хоткей, подсказки, воздух, индикатор, слот. Числа посчитаны
    /// руками (13 + 1 + 9 + 2 + 23 + 8 + 9 + 2 + 11 = 78 = ширина − запас у стены).
    #[test]
    fn footer_puts_git_before_the_right_slot() {
        let app = footer_app();

        assert_eq!(
            footer_line(&app, 80),
            format!(
                ">> Обсуждение Shift+Tab  ? подсказки · / команды{}git: main{}чат: Claude  ",
                " ".repeat(8),
                " ".repeat(2)
            )
        );
    }

    /// Слот шире текущего текста (идёт переход со старого сегмента) — добивка пробелами
    /// идёт ПОСЛЕ индикатора: сам индикатор не съезжает.
    #[test]
    fn wider_previous_segment_pads_the_slot_after_git() {
        let mut app = footer_app();
        app.footer_right_previous_text = Some("усилие: Codex xhigh".to_string()); // 19 колонок

        // 38 + 20 (воздух) + 9 (git) + 2 (зазор) + 8 (добивка слота) + 11 = 98 = 100 − 2.
        assert_eq!(
            footer_line(&app, 100),
            format!(
                ">> Обсуждение Shift+Tab  ? подсказки · / команды{}git: main{}чат: Claude  ",
                " ".repeat(20),
                " ".repeat(2 + 8)
            )
        );
    }

    /// Узко: рядом с индикатором не влезает даже первый пункт подсказок — индикатор уходит,
    /// подсказки усекаются, правый слот остаётся на месте.
    #[test]
    fn narrow_footer_drops_git_and_keeps_the_slot() {
        let app = footer_app();

        assert_eq!(
            footer_line(&app, 50),
            format!(">> Обсуждение Shift+Tab  ? подсказ…{}чат: Claude  ", "  ")
        );
    }

    /// Уведомление занимает футер, но индикатор постоянный — он остаётся у правого края.
    #[test]
    fn notice_keeps_git_at_the_right_edge() {
        let mut app = footer_app();
        app.show_footer_notice("Сохранено");

        // 9 (текст) + 58 (воздух) + 2 (зазор) + 9 (git) = 78.
        assert_eq!(
            footer_line(&app, 80),
            format!("Сохранено{}git: main  ", " ".repeat(58 + 2))
        );
    }

    /// Длинное уведомление режется по бюджету МИНУС индикатор: git не вытесняется за край.
    #[test]
    fn long_notice_is_truncated_to_leave_room_for_git() {
        let mut app = footer_app();
        app.show_footer_notice("a".repeat(100));

        // budget 78 − git_total 11 = 67 колонок под текст: 66 символов и «…».
        assert_eq!(
            footer_line(&app, 80),
            format!("{}…  git: main  ", "a".repeat(66))
        );
    }

    /// Граница: бюджет РАВЕН ширине индикатора с зазором — места нет, рисуем одно уведомление.
    /// Иначе (при `>=`) от текста уведомления не осталось бы ничего.
    #[test]
    fn notice_drops_git_when_the_budget_only_matches_it() {
        let mut app = footer_app();
        app.show_footer_notice("ok");

        // width 13 → budget 11 == display_width("git: main") + 2.
        assert_eq!(footer_line(&app, 13), format!("ok{}", " ".repeat(11)));
    }

    /// Без репозитория уведомление занимает футер целиком.
    #[test]
    fn notice_without_git_fills_the_footer() {
        let mut app = footer_app();
        app.git_ref = None;
        app.show_footer_notice("Готово");

        assert_eq!(footer_line(&app, 80), format!("Готово{}", " ".repeat(74)));
    }

    /// Уведомление живёт 2 секунды: устаревшее не рисуется, футер возвращается к обычному виду.
    #[test]
    fn expired_notice_gives_the_footer_back() {
        let mut app = footer_app();
        let stale = Instant::now()
            .checked_sub(Duration::from_secs(3))
            .expect("часы монотонны");
        app.footer_notice = Some(("Сохранено".to_string(), stale));

        assert_eq!(
            footer_line(&app, 80),
            format!(
                ">> Обсуждение Shift+Tab  ? подсказки · / команды{}git: main{}чат: Claude  ",
                " ".repeat(8),
                " ".repeat(2)
            )
        );
    }

    /// Текст строки блока — как его увидит терминал.
    fn text_of(line: &Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    /// Покой: воздух, две линейки композера, поле ввода, футер — ровно 5 строк, курсор
    /// в поле ввода сразу за приглашением.
    #[test]
    fn idle_block_is_air_composer_and_footer() {
        let app = footer_app();

        let (lines, cur_row, cur_col) = build_dynamic(&app, 80, 24);

        assert_eq!((lines.len(), cur_row, cur_col), (5, 2, 2));
        assert_eq!(text_of(&lines[0]).trim(), "");
        assert!(
            text_of(&lines[2]).starts_with('›'),
            "поле ввода на строке 2"
        );
        assert!(text_of(&lines[4]).contains("Обсуждение"), "футер снизу");
    }

    /// Терминал в 4 строки: блок не проваливается ниже «композер + футер» — иначе поле
    /// ввода обрезалось бы, а курсор ушёл бы за нижнюю линейку.
    #[test]
    fn tiny_terminal_keeps_composer_and_footer() {
        let app = footer_app();

        let (lines, cur_row, cur_col) = build_dynamic(&app, 80, 4);

        assert_eq!((lines.len(), cur_row, cur_col), (4, 2, 2));
    }

    /// Реплика текущего рана: ведущая пустая строка бокса срезана — воздух уже даёт gap_top,
    /// второй пустой строки быть не должно.
    #[test]
    fn live_turn_drops_its_leading_blank_line() {
        let mut app = footer_app();
        app.live_turn = Some("привет".to_string());

        let (lines, cur_row, _) = build_dynamic(&app, 80, 24);

        assert_eq!((lines.len(), cur_row), (6, 3));
        assert!(
            text_of(&lines[1]).starts_with("привет"),
            "реплика сразу под воздухом: {:?}",
            text_of(&lines[1])
        );
    }

    /// Ран без единого токена: над вводом только лоадер (никакого пустого «⏺») и ровно
    /// один отступ сверху — второй был бы двойным воздухом.
    #[test]
    fn running_without_answer_shows_loader_only() {
        let mut app = footer_app();
        app.running = true;

        let (lines, cur_row, _) = build_dynamic(&app, 80, 24);

        assert_eq!((lines.len(), cur_row), (7, 4));
        let block: String = lines.iter().map(text_of).collect();
        assert!(
            !block.contains('⏺'),
            "пустого ответа в блоке нет: {block:?}"
        );
    }

    /// Ран с токенами: ответ рисуется как «⏺ …», между ним и лоадером — отступ.
    #[test]
    fn running_with_answer_puts_air_between_answer_and_loader() {
        let mut app = footer_app();
        app.running = true;
        app.live_answer = "привет".to_string();

        let (lines, _, _) = build_dynamic(&app, 80, 24);

        assert!(
            text_of(&lines[2]).starts_with("⏺ привет"),
            "ответ в верхнем слоте: {:?}",
            text_of(&lines[2])
        );
        assert_eq!(text_of(&lines[3]).trim(), "", "отступ перед лоадером");
        // воздух + ответ(2) + отступ + лоадер + отступ + композер(3) + футер(1)
        assert_eq!(lines.len(), 10);
    }

    /// Длинный ответ в высоком окне: весь верхний слот влезает, курсор едет вниз вместе
    /// с ним. Проверяет арифметику высоты блока и строки курсора.
    #[test]
    fn long_answer_grows_the_block_and_moves_the_cursor_down() {
        let mut app = footer_app();
        app.running = true;
        app.live_answer = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj".to_string();

        let (lines, cur_row, cur_col) = build_dynamic(&app, 80, 24);

        assert_eq!((lines.len(), cur_row, cur_col), (19, 16, 2));
        assert!(
            text_of(&lines[2]).starts_with("⏺ a"),
            "ответ виден целиком: {:?}",
            text_of(&lines[2])
        );
        assert!(
            text_of(&lines[16]).starts_with('›'),
            "курсор стоит на поле ввода"
        );
    }

    /// Тот же ответ в НИЗКОМ окне: место под верхний слот считается за вычетом воздуха,
    /// композера и футера. Ошибка в этом резерве уводит курсор в футер.
    #[test]
    fn long_answer_in_a_short_window_is_capped_by_the_reserved_rows() {
        let mut app = footer_app();
        app.running = true;
        app.live_answer = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj".to_string();

        let (lines, cur_row, cur_col) = build_dynamic(&app, 80, 12);

        assert_eq!((lines.len(), cur_row, cur_col), (11, 8, 2));
        assert!(text_of(&lines[8]).starts_with('›'), "курсор на поле ввода");
    }

    /// Открытая панель: футер гаснет (панель сама несёт контекст), блок вырастает на её
    /// высоту, содержимое палитры нарисовано.
    #[test]
    fn open_panel_hides_the_footer_and_grows_the_block() {
        let mut app = footer_app();
        app.input = "/".to_string();
        app.cursor = 1;

        let (lines, cur_row, cur_col) = build_dynamic(&app, 80, 24);

        assert_eq!((lines.len(), cur_row, cur_col), (16, 2, 3));
        let block: String = lines.iter().map(text_of).collect();
        assert!(
            block.contains("/brainstorm"),
            "палитра нарисована: {block:?}"
        );
        assert!(!block.contains("Обсуждение"), "футер спрятан под панелью");
    }

    /// Многострочный ввод: курсор считается по ПЕРЕНЕСЁННЫМ строкам и колонкам, а не по
    /// смещению в тексте.
    #[test]
    fn cursor_follows_the_wrapped_input() {
        let mut app = footer_app();
        app.input = "ab\ncde".to_string();
        app.cursor = 6;

        let (lines, cur_row, cur_col) = build_dynamic(&app, 80, 24);

        assert_eq!((lines.len(), cur_row, cur_col), (6, 3, 5));
    }

    /// Ширина в одну колонку: курсор не вылезает за правый край.
    #[test]
    fn cursor_column_never_leaves_the_screen() {
        let app = footer_app();

        let (_, _, cur_col) = build_dynamic(&app, 1, 10);

        assert_eq!(cur_col, 0);
    }

    /// Буфер → строки: одинаковые стили схлопываются в спан, РАЗНЫЕ рвут его. Иначе футер
    /// уехал бы в один серый спан и подсветка исчезла.
    #[test]
    fn buffer_lines_split_spans_by_style() {
        let app = footer_app();

        let (lines, _, _) = build_dynamic(&app, 80, 24);

        let footer = lines.last().expect("блок не пуст");
        assert!(footer.spans.len() > 1, "футер разбит по стилям: {footer:?}");
        assert!(
            footer.spans.iter().all(|span| !span.content.is_empty()),
            "пустых спанов не бывает: {footer:?}"
        );
        assert!(
            footer.spans.iter().any(|span| span.style.fg.is_some()),
            "цвет сохранён: {footer:?}"
        );
        assert_eq!(
            text_of(footer),
            footer_line(&app, 80),
            "текст строки собран без потерь"
        );
    }

    /// Первый кадр: курсор прячется, печатается история и весь блок, курсор ставится в поле
    /// ввода и ПОКАЗЫВАЕТСЯ обратно. Перевод строки — ровно один на строку истории и на
    /// каждую строку блока, кроме последней (иначе блок «сползал» бы вниз каждый кадр).
    #[test]
    fn first_frame_prints_history_and_the_block() {
        let mut app = footer_app();
        app.transcript.push("привет".to_string());
        let mut renderer = LiveRenderer::new();

        let out = frame(&mut renderer, &mut app, 80, 24);

        assert!(out.starts_with("\u{1b}[?25l"), "курсор спрятан: {out:?}");
        assert!(out.ends_with("\u{1b}[?25h"), "курсор возвращён: {out:?}");
        assert!(out.contains("привет"), "история напечатана");
        assert!(out.contains("Обсуждение"), "футер напечатан");
        assert!(out.contains("\u{1b}[38;5;97m"), "цвет спанов на месте");
        assert_eq!(
            out.matches("\r\n").count(),
            5,
            "1 строка истории + 5−1 блока"
        );
        // хвост: в начало строки → вверх на (последняя − строка курсора) → вправо на колонку
        assert!(
            out.ends_with("\u{1b}[1G\u{1b}[2A\u{1b}[2C\u{1b}[?25h"),
            "курсор поставлен в поле ввода: {out:?}"
        );
        assert_eq!(app.scrollback_count, 1, "история ушла в скроллбэк");
    }

    /// Второй кадр без изменений не пишет НИЧЕГО: иначе футер мерцал бы на каждом тике.
    #[test]
    fn unchanged_frame_writes_nothing() {
        let mut app = footer_app();
        app.transcript.push("привет".to_string());
        let mut renderer = LiveRenderer::new();

        assert!(!frame(&mut renderer, &mut app, 80, 24).is_empty());
        assert_eq!(frame(&mut renderer, &mut app, 80, 24), "");
    }

    /// Новая строка ленты уходит в скроллбэк РОВНО ОДИН РАЗ: второй кадр печатает только её,
    /// старую историю не повторяет.
    #[test]
    fn new_history_is_printed_once() {
        let mut app = footer_app();
        app.transcript.push("первая".to_string());
        let mut renderer = LiveRenderer::new();
        frame(&mut renderer, &mut app, 80, 24);

        app.transcript.push("вторая".to_string());
        let out = frame(&mut renderer, &mut app, 80, 24);

        assert!(out.contains("вторая"), "новая строка напечатана: {out:?}");
        assert!(!out.contains("первая"), "старая — не повторяется: {out:?}");
        assert_eq!(app.scrollback_count, 2);
        // Структурный путь: спуститься на низ старого блока → на его верх → стереть вниз.
        assert!(
            out.starts_with("\u{1b}[?25l\u{1b}[2B\u{1b}[1G\u{1b}[4A\u{1b}[J"),
            "старый блок стёрт с его верхней строки: {out:?}"
        );
        assert_eq!(
            out.matches("\r\n").count(),
            5,
            "1 строка истории + 5−1 блока"
        );

        // и третий кадр (уже без новостей) снова молчит
        assert_eq!(frame(&mut renderer, &mut app, 80, 24), "");
    }

    /// Дифф: меняем ОДНУ строку блока — в терминал уходит только она. Ни рамок композера,
    /// ни очистки экрана: остальные строки не трогаем, поэтому футер не мерцает.
    #[test]
    fn diff_frame_touches_only_the_changed_line() {
        let mut app = footer_app();
        let mut renderer = LiveRenderer::new();
        frame(&mut renderer, &mut app, 80, 24);

        app.show_footer_notice("Сохранено");
        let out = frame(&mut renderer, &mut app, 80, 24);

        assert!(
            out.contains("Сохранено"),
            "изменившаяся строка ушла: {out:?}"
        );
        assert!(
            !out.contains('─'),
            "рамки композера не перерисованы: {out:?}"
        );
        assert!(!out.contains("\u{1b}[J"), "экран не стирался: {out:?}");
        assert!(
            !out.contains("\r\n"),
            "дифф не двигает блок переводами строк"
        );
        assert!(
            out.starts_with("\u{1b}[?25l\u{1b}[2B\u{1b}[1G\u{1b}[4A"),
            "встали на верх блока: {out:?}"
        );
        // спуск ровно по одному разу на каждую строку блока, кроме последней
        assert_eq!(out.matches("\u{1b}[1B").count(), 4);
        assert!(out.ends_with("\u{1b}[1G\u{1b}[2A\u{1b}[2C\u{1b}[?25h"));
    }

    /// Курсор упирается в нижнюю строку блока (панель съела всё место): нулевых сдвигов
    /// в выводе быть не должно — `ESC[0A`/`ESC[0B`/`ESC[0C` терминал понимает как сдвиг на 1.
    #[test]
    fn zero_moves_are_never_emitted() {
        let mut app = footer_app();
        app.input = "/".to_string();
        app.cursor = 1;
        let mut renderer = LiveRenderer::new();

        let first = frame(&mut renderer, &mut app, 80, 4);
        assert!(!first.contains("\u{1b}[0A"), "первый кадр: {first:?}");

        // дифф-кадр: курсор на последней строке блока → шага вниз быть не должно
        app.input = "/c".to_string();
        app.cursor = 2;
        let diff = frame(&mut renderer, &mut app, 80, 4);
        assert!(diff.contains("/c"), "строка ввода перерисована: {diff:?}");
        assert!(!diff.contains("\u{1b}[0B"), "дифф: {diff:?}");
        assert!(!diff.contains("\u{1b}[0A"), "дифф: {diff:?}");

        // структурный кадр из того же положения
        app.transcript.push("x".to_string());
        let structural = frame(&mut renderer, &mut app, 80, 4);
        assert!(
            !structural.contains("\u{1b}[0B"),
            "структурный: {structural:?}"
        );
        assert!(
            structural.starts_with("\u{1b}[?25l\u{1b}[1G\u{1b}[2A\u{1b}[J"),
            "структурный встаёт на верх блока: {structural:?}"
        );

        // и уход вниз из того же положения
        let mut buf: Vec<u8> = Vec::new();
        renderer.leave_below_to(&mut buf).expect("уход вниз");
        let leave = String::from_utf8_lossy(&buf);
        assert!(!leave.contains("\u{1b}[0B"), "leave_below: {leave:?}");

        // курсор в колонке 0 (ширина в одну колонку) — сдвига вправо тоже нет
        let mut narrow = footer_app();
        let mut renderer = LiveRenderer::new();
        let out = frame(&mut renderer, &mut narrow, 1, 10);
        assert!(!out.contains("\u{1b}[0C"), "нулевой сдвиг вправо: {out:?}");
        assert!(out.ends_with("\u{1b}[1G\u{1b}[2A\u{1b}[?25h"), "{out:?}");
    }

    /// `/clear`: стираем экран И нативный скроллбэк, блок рисуем заново с нуля. История
    /// уже в скроллбэке — её не перепечатываем.
    #[test]
    fn pending_clear_screen_wipes_the_terminal() {
        let mut app = footer_app();
        app.transcript.push("привет".to_string());
        let mut renderer = LiveRenderer::new();
        frame(&mut renderer, &mut app, 80, 24);

        app.pending_clear_screen = true;
        let out = frame(&mut renderer, &mut app, 80, 24);

        assert!(
            out.starts_with("\u{1b}[2J\u{1b}[3J\u{1b}[1;1H"),
            "экран и скроллбэк: {out:?}"
        );
        assert!(!app.pending_clear_screen, "запрос погашен");
        assert!(out.contains("Обсуждение"), "блок перерисован");
        assert!(!out.contains("привет"), "история не печатается заново");
        // кэш позиций сброшен: блок рисуется от текущей строки, без сдвигов вверх
        assert!(
            out.contains("\u{1b}[?25l\u{1b}[1G\u{1b}[J"),
            "кэш позиций сброшен: {out:?}"
        );
        assert_eq!(app.scrollback_count, 1, "счётчик истории цел");
    }

    /// Ресайз: терминал перелил историю под новую ширину — чистим экран, скроллбэк И счётчик,
    /// историю печатаем заново, иначе живой блок дублируется.
    #[test]
    fn pending_full_redraw_reprints_history_from_scratch() {
        let mut app = footer_app();
        app.transcript.push("привет".to_string());
        let mut renderer = LiveRenderer::new();
        frame(&mut renderer, &mut app, 80, 24);

        app.pending_full_redraw = true;
        let out = frame(&mut renderer, &mut app, 60, 24);

        assert!(
            out.starts_with("\u{1b}[2J\u{1b}[3J\u{1b}[1;1H"),
            "экран и скроллбэк: {out:?}"
        );
        assert!(!app.pending_full_redraw, "запрос погашен");
        assert!(
            out.contains("привет"),
            "история перепечатана под новую ширину"
        );
        assert_eq!(app.scrollback_count, 1, "счётчик пересобран");
    }

    /// `invalidate` заставляет следующий кадр перерисоваться целиком (после внешней команды
    /// экран под нами уже не тот, что в кэше).
    #[test]
    fn invalidate_forces_a_full_repaint() {
        let mut app = footer_app();
        let mut renderer = LiveRenderer::new();
        frame(&mut renderer, &mut app, 80, 24);
        assert_eq!(
            frame(&mut renderer, &mut app, 80, 24),
            "",
            "кадр без изменений молчит"
        );

        renderer.invalidate();
        let out = frame(&mut renderer, &mut app, 80, 24);

        assert!(
            out.contains("Обсуждение"),
            "блок перерисован целиком: {out:?}"
        );
    }

    /// Заголовок окна ставится один раз на смену имени: безымянный чат — пусто, названный —
    /// имя, повтор — тишина (иначе терминал моргал бы заголовком каждый кадр).
    #[test]
    fn terminal_title_is_set_once_per_change() {
        let mut app = footer_app();
        app.chat_title_custom = true;
        app.chat_title = "Мой чат".to_string();
        let mut renderer = LiveRenderer::new();

        let out = frame(&mut renderer, &mut app, 80, 24);

        assert!(
            out.starts_with("\u{1b}]0;Мой чат\u{7}"),
            "заголовок выставлен: {out:?}"
        );
        assert_eq!(
            frame(&mut renderer, &mut app, 80, 24),
            "",
            "повтор ничего не пишет"
        );
        assert_eq!(terminal_window_title(&app), "Мой чат");

        let plain = footer_app();
        assert_eq!(
            terminal_window_title(&plain),
            "",
            "безымянный чат заголовок не трогает"
        );
    }

    /// `leave_below_to`: на непроинициализированном рендерере молчит, после кадра — стирает
    /// живой блок с его верхней строки и возвращает курсор.
    #[test]
    fn leave_below_erases_the_block_only_when_started() {
        let mut renderer = LiveRenderer::new();
        let mut buf: Vec<u8> = Vec::new();
        renderer.leave_below_to(&mut buf).expect("уход вниз");
        assert!(buf.is_empty(), "без кадра писать нечего: {buf:?}");

        let mut app = footer_app();
        frame(&mut renderer, &mut app, 80, 24);
        let mut buf: Vec<u8> = Vec::new();
        renderer.leave_below_to(&mut buf).expect("уход вниз");
        let out = String::from_utf8_lossy(&buf);

        assert_eq!(out, "\u{1b}[2B\u{1b}[1G\u{1b}[4A\u{1b}[J\u{1b}[?25h");

        // блок стёрт и забыт: повторный уход уже ничего не пишет
        let mut again: Vec<u8> = Vec::new();
        renderer.leave_below_to(&mut again).expect("уход вниз");
        assert!(again.is_empty(), "второй раз стирать нечего: {again:?}");
    }

    /// Выход из приложения: экран И нативный скроллбэк чистые, курсор показан.
    #[test]
    fn clear_for_exit_wipes_screen_and_scrollback() {
        let mut app = footer_app();
        let mut renderer = LiveRenderer::new();
        frame(&mut renderer, &mut app, 80, 24);

        let mut buf: Vec<u8> = Vec::new();
        renderer.clear_for_exit_to(&mut buf, &app).expect("выход");
        let out = String::from_utf8_lossy(&buf);

        assert_eq!(out, "\u{1b}[1G\u{1b}[2J\u{1b}[3J\u{1b}[1;1H\u{1b}[?25h");

        let mut after: Vec<u8> = Vec::new();
        renderer.leave_below_to(&mut after).expect("уход вниз");
        assert!(after.is_empty(), "живого блока больше нет: {after:?}");
    }

    /// Стиль спана уходит в терминал: цвет, жирность — и сброс после каждого спана.
    #[test]
    fn queue_line_writes_colors_and_attributes() {
        let line = Line::from(vec![
            Span::styled(
                "жирный",
                Style::default()
                    .fg(Color::LightMagenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" обычный"),
        ]);

        let mut buf: Vec<u8> = Vec::new();
        queue_line(&mut buf, &line).expect("строка в память");
        let out = String::from_utf8_lossy(&buf);

        assert!(out.contains("\u{1b}[38;5;13m"), "цвет спана: {out:?}");
        assert!(out.contains("\u{1b}[1m"), "жирность: {out:?}");
        assert!(out.contains("жирный"), "текст: {out:?}");
        assert!(
            out.ends_with("\u{1b}[0m\u{1b}[0m"),
            "стиль сброшен: {out:?}"
        );
    }

    #[test]
    fn sanitize_strips_escape_and_control_keeps_text() {
        // ESC/OSC/цвет/CR/BEL — вырезаются (инъекция в терминал невозможна).
        let evil = "НАЧАЛО\u{1b}[31mКРАСНЫЙ\u{1b}]0;PWNED\u{7}\rКОНЕЦ";
        let clean = sanitize_terminal_text(evil);
        assert!(!clean.contains('\u{1b}'), "ESC должен быть убран");
        assert!(!clean.contains('\u{7}') && !clean.contains('\r'));
        assert_eq!(clean, "НАЧАЛО[31mКРАСНЫЙ]0;PWNEDКОНЕЦ");
        // Обычный текст, кириллица, рамки и табы — нетронуты (и без аллокации).
        let safe = "│ ответ\tкод ╭─╮ Ω";
        assert!(matches!(
            sanitize_terminal_text(safe),
            std::borrow::Cow::Borrowed(_)
        ));
        assert_eq!(sanitize_terminal_text(safe), safe);
    }

    #[test]
    fn queue_rich_line_wraps_links_and_still_sanitizes_content() {
        // Спан-ссылка (индекс 1) + спан с инъекцией ESC/OSC в КОНТЕНТЕ.
        let line = Line::from(vec![
            Span::raw("see "),
            Span::raw("src/app.rs"),
            Span::raw("\u{1b}]0;PWNED\u{7} tail"),
        ]);
        let rich = RichLine {
            line,
            links: vec![SpanLink {
                span: 1,
                url: "vscode://file/x:1:1".to_string(),
            }],
        };
        let mut buf: Vec<u8> = Vec::new();
        queue_rich_line(&mut buf, &rich).unwrap();
        let out = String::from_utf8_lossy(&buf);

        // OSC 8 обрамляет ИМЕННО свой спан: открытие вплотную перед путём, а не перед соседом.
        assert!(
            out.contains("\u{1b}]8;;vscode://file/x:1:1\u{1b}\\src/app.rs\u{1b}]8;;\u{1b}\\"),
            "OSC8 обрамляет путь доверенным URL: {out:?}"
        );
        assert!(
            !out.contains("\u{1b}]8;;vscode://file/x:1:1\u{1b}\\see "),
            "ссылка не на соседе"
        );
        // Контентный OSC (инъекция) вырезан — ESC-форма отсутствует.
        assert!(
            !out.contains("\u{1b}]0;PWNED"),
            "контентный OSC не должен пройти: {out:?}"
        );
    }

    #[test]
    fn terminal_title_only_set_for_named_chats() {
        // Назван явно → ставим ТОЛЬКО имя (путь/процесс/размер рисует терминал).
        assert_eq!(terminal_title_for(true, "myproject"), "myproject");
        // Безымянный → пусто: clave заголовок не трогает, нет дублирования пути/clave.
        assert_eq!(terminal_title_for(false, "chat-123"), "");
        // Пустое имя даже при custom → пусто.
        assert_eq!(terminal_title_for(true, "  "), "");
    }

    #[test]
    fn terminal_title_strips_control_sequences() {
        assert_eq!(
            terminal_title_for(true, "ok\u{1b}]0;pwn\u{7}\rtitle"),
            "ok]0;pwntitle"
        );
    }

    // ─────────────────── ЗАПИСЬ В НАСТОЯЩИЙ ТЕРМИНАЛ ───────────────────
    //
    // `render`, `leave_below` и `clear_for_exit` — тонкие обёртки: лочат `io::stdout()` и
    // делегируют в `*_to`. Их можно было заменить на `Ok(())`, и НИ ОДИН тест не заметил бы —
    // всё покрытие живёт на `*_to`, куда мы подсовываем `Vec<u8>`. А значит, приложение могло
    // бы не рисовать НИЧЕГО (пустой экран), не стирать живой блок перед внешней командой
    // (вывод команды налез бы на блок) и не убирать беседу с экрана при выходе.
    //
    // Проверить факт записи в НАСТОЯЩИЙ stdout изнутри теста нельзя: он уходит мимо перехвата
    // харнесса, а перенаправлять fd 1 на лету — значит гонять глобальное состояние процесса и
    // ломать соседние тесты. Поэтому смотрим со стороны: запускаем случай в ДОЧЕРНЕМ процессе
    // и ловим его вывод.

    const RENDER_CASE: &str = "CLAVE_TEST_RENDER_CASE";
    const RENDER_SELF: &str = "render::tests::the_wrappers_really_write_to_the_terminal";

    fn run_render_case(case: &str) -> String {
        let exe = std::env::current_exe().expect("путь к тестовому бинарю");
        let out = std::process::Command::new(exe)
            .args([RENDER_SELF, "--exact", "--nocapture", "--test-threads=1"])
            .env(RENDER_CASE, case)
            .output()
            .expect("дочерний тест не запустился");
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr)
    }

    #[test]
    fn the_wrappers_really_write_to_the_terminal() {
        match std::env::var(RENDER_CASE).ok().as_deref() {
            // Ребёнок: рисуем кадр по-настоящему. В stdout обязана уйти реплика из ленты.
            Some("render") => {
                let mut app = render_app();
                app.transcript.push("◆ МАРКЕР_ЛЕНТЫ".to_string());
                let mut renderer = LiveRenderer::new();
                renderer.render(&mut app, 80, 24).expect("рендер");
            }
            // Ребёнок: стираем живой блок. Состояние ставим руками — важен ФАКТ записи.
            Some("leave") => {
                let mut renderer = LiveRenderer::new();
                renderer.started = true;
                renderer.prev_height = 4;
                renderer.cursor_above = 1;
                renderer.leave_below().expect("leave_below");
            }
            // Ребёнок: чистый выход — экран И нативный скроллбэк.
            Some("exit") => {
                let app = render_app();
                let mut renderer = LiveRenderer::new();
                renderer.clear_for_exit(&app).expect("clear_for_exit");
            }
            _ => {
                let drawn = run_render_case("render");
                assert!(
                    drawn.contains("1 passed"),
                    "дочерний случай «render» обязан РЕАЛЬНО прогнаться; коду возврата тут верить \
                     нельзя — с опечаткой в фильтре ребёнок гоняет ноль тестов и выходит нулём:\n{drawn}"
                );
                assert!(
                    drawn.contains("МАРКЕР_ЛЕНТЫ"),
                    "render не написал в НАСТОЯЩИЙ stdout ничего — значит его можно заменить \
                     пустышкой, и приложение будет показывать пустой экран:\n{drawn:?}"
                );

                let left = run_render_case("leave");
                assert!(
                    left.contains("1 passed"),
                    "случай «leave» не прогнался:\n{left}"
                );
                assert!(
                    left.contains("\u{1b}[J") && left.contains("\u{1b}[?25h"),
                    "leave_below не стёр живой блок и не вернул курсор — вывод внешней команды \
                     налез бы на блок:\n{left:?}"
                );

                let exited = run_render_case("exit");
                assert!(
                    exited.contains("1 passed"),
                    "случай «exit» не прогнался:\n{exited}"
                );
                assert!(
                    // Purge — стирание НАТИВНОГО скроллбэка терминала. Его шлёт только выход:
                    // обычный кадр этой последовательности не выдаёт.
                    exited.contains("\u{1b}[3J"),
                    "clear_for_exit не стёр скроллбэк — беседа осталась бы в терминале после \
                     закрытия приложения:\n{exited:?}"
                );
            }
        }
    }
}
