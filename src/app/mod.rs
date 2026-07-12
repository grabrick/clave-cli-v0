use crate::prelude::*;
use crate::*;

mod ask;
mod chats;
mod commands;
mod config;
mod dev;
mod editor;
mod effort;
mod events;
mod external;
mod footer;
mod onboarding;
mod plan;
mod runs;
mod search;
mod settings;
mod tandem;

pub(crate) use ask::*;
pub(crate) use config::*;
pub(crate) use effort::*;
pub(crate) use events::*;
pub(crate) use external::*;
pub(crate) use onboarding::*;
pub(crate) use plan::*;
pub(crate) use settings::*;

pub(crate) struct App {
    pub(crate) mode: Mode,
    pub(crate) direct_provider: Provider,
    pub(crate) chat_mode: ChatMode,
    pub(crate) theme: Theme,
    pub(crate) lang: Language,
    pub(crate) rounds: usize,
    pub(crate) work_dir: String,
    pub(crate) out_dir: String,
    pub(crate) config_path: PathBuf,
    pub(crate) history_path: PathBuf,
    pub(crate) chats_dir: PathBuf,
    pub(crate) chat_id: String,
    pub(crate) chat_path: PathBuf,
    pub(crate) chat_title: String,
    pub(crate) chat_title_custom: bool,
    /// Куда открывать путь по Cmd+клику (OSC 8). Из конфига или авто-детектом.
    pub(crate) path_link_target: PathTarget,
    pub(crate) onboarding: Option<Onboarding>,
    pub(crate) pending_external: Option<ExternalCommand>,
    pub(crate) input: String,
    pub(crate) cursor: usize,
    pub(crate) transcript: Vec<String>,
    /// Сколько первых строк `transcript` уже вытеснено в нативный скроллбэк
    /// терминала (через insert_before). Хвост `transcript[scrollback_count..]`
    /// живёт в нижнем viewport и перерисовывается на месте.
    pub(crate) scrollback_count: usize,
    /// Состояние рендера (code-block) на границе вытеснения — чтобы хвост
    /// рисовался с корректной подсветкой, не пересчитывая всю историю.
    pub(crate) flush_state: TranscriptRenderState,
    /// Запрос на полную очистку терминала (экран + нативный скроллбэк): ставится
    /// при сбросе ленты (/clear, /new, /resume), исполняется рендером.
    pub(crate) pending_clear_screen: bool,
    /// Запрос на полную перерисовку (история + живой блок) с чистого листа: ставится
    /// при ресайзе терминала — геометрия сменилась, кэш позиций рендера устарел,
    /// иначе живой блок «съезжает» и дублируется (классика после сна ПК).
    pub(crate) pending_full_redraw: bool,
    /// Текст ответа, приходящий токен-стримом и показываемый вживую (claude). По
    /// завершении заменяется зафиксированным финальным текстом. Пусто → стрима не было
    /// (codex / нет partial-messages), тогда работает «печатная машинка» (reveal).
    pub(crate) live_answer: String,
    /// Рассуждение (extended thinking) claude, приходящее до ответа — показывается в
    /// лоадере вживую, чтобы ощущалось «модель думает». Очищается вместе с ответом.
    pub(crate) live_reasoning: String,
    /// Строки ответа, накопленные до завершения (потом «печатаются» через reveal).
    pub(crate) reveal_buffer: Vec<String>,
    /// Активная плавная отрисовка ответа («печатная машинка») или None.
    pub(crate) reveal: Option<Reveal>,
    /// Текст текущего чат-запроса — чтобы при отмене (Ctrl+C) вернуть его в инпут,
    /// а не потерять. Ставится при старте чата, снимается по завершении.
    pub(crate) restore_on_cancel: Option<String>,
    /// Реплика пользователя текущего чат-рана («◆ …»), НЕ зафиксированная в ленте:
    /// живёт в живом блоке, пока идёт ран. На успехе уходит в скроллбэк, на отмене
    /// исчезает без следа (иначе она уже была бы напечатана в нативном скроллбэке).
    pub(crate) live_turn: Option<String>,
    /// Активный inline-селектор выбора (блок clave-ask от модели) или None.
    pub(crate) ask: Option<AskState>,
    /// Разобранный запрос выбора, ждущий завершения «печати» прозы перед показом.
    pub(crate) ask_prompt_pending: Option<AskPrompt>,
    pub(crate) status: String,
    pub(crate) last_run: Option<String>,
    pub(crate) last_run_duration: Option<Duration>,
    pub(crate) running: bool,
    /// Текущий прогон — самопиление (`/dev`). У него СВОЯ семантика кода выхода
    /// (0 сошлось, 1 не сошлось, 2 дерево не чистое), поэтому финал рендерится иначе:
    /// «не сошлось» — это исход, а не поломка, и ошибкой его звать нельзя.
    pub(crate) dev_run: bool,
    pub(crate) run_started_at: Option<Instant>,
    pub(crate) run_label: String,
    pub(crate) run_token_estimate: Option<usize>,
    pub(crate) run_activity: VecDeque<String>,
    pub(crate) cancel_tx: Option<Sender<()>>,
    pub(crate) last_ctrl_c_at: Option<Instant>,
    pub(crate) footer_notice: Option<(String, Instant)>,
    pub(crate) footer_right_text: String,
    pub(crate) footer_right_previous_text: Option<String>,
    pub(crate) footer_right_changed_at: Option<Instant>,
    pub(crate) should_quit: bool,
    pub(crate) history: Vec<String>,
    pub(crate) history_index: Option<usize>,
    /// Незавершённый ввод, сохранённый при входе в историю (для возврата по Down).
    pub(crate) history_draft: Option<String>,
    pub(crate) selected_suggestion: usize,
    pub(crate) command_palette_opened_at: Option<Instant>,
    pub(crate) command_palette_query: String,
    pub(crate) overlay: Overlay,
    pub(crate) chats_picker: Vec<ChatSummary>,
    pub(crate) chats_index: usize,
    pub(crate) search_query: String,
    pub(crate) search_index: usize,
    pub(crate) last_chat_message: Option<String>,
    pub(crate) pending_plan: Option<PendingPlan>,
    pub(crate) plan_flow: PlanFlow,
    pub(crate) pending_messages: VecDeque<String>,
    pub(crate) effort_original: Option<EffortSnapshot>,
    pub(crate) effort_focus: usize,
    pub(crate) settings_original: Option<SettingsSnapshot>,
    pub(crate) settings_focus: usize,
    pub(crate) effort_index: usize,
    pub(crate) codex_effort_index: usize,
    pub(crate) claude_effort_index: usize,
    pub(crate) linked_effort_split: bool,
    pub(crate) tx: Sender<WorkerEvent>,
    pub(crate) rx: Receiver<WorkerEvent>,
    pub(crate) usage: SessionUsage,
}

impl App {
    pub(crate) fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let config_path = config_path();
        let history_path = history_path();
        let chats_dir = chats_dir();
        let mut config = load_config(&config_path);
        if env::var("CLAVE_SKIP_ONBOARDING").ok().as_deref() == Some("1") {
            config.onboarding_done = true;
        }
        config.effort_index = normalize_common_effort_index(config.effort_index);
        config.codex_effort_index =
            normalize_provider_effort_index("codex", config.codex_effort_index);
        config.claude_effort_index =
            normalize_provider_effort_index("claude", config.claude_effort_index);
        let onboarding = if config.onboarding_done {
            None
        } else {
            Some(Onboarding::new(config.mode))
        };

        let (chat_id, chat_path, transcript) =
            restore_or_create_chat(&chats_dir, config.last_chat_id.as_deref(), config.lang);
        let chat_title_custom = read_chat_title(&chat_path).is_some();
        let chat_title = chat_display_title(&chat_path, &transcript, &chat_id);
        let history = load_history(&history_path).unwrap_or_default();
        let last_run = find_last_run(&transcript);

        Self {
            mode: config.mode,
            direct_provider: config.direct_provider,
            chat_mode: ChatMode::default(),
            theme: config.theme,
            lang: config.lang,
            rounds: config.rounds,
            work_dir: config.work_dir,
            out_dir: config.out_dir,
            config_path,
            history_path,
            chats_dir,
            chat_id,
            chat_path,
            chat_title,
            chat_title_custom,
            path_link_target: config
                .path_link_target
                .unwrap_or_else(|| auto_default(editor_installed)),
            onboarding,
            pending_external: None,
            input: String::new(),
            cursor: 0,
            transcript,
            scrollback_count: 0,
            flush_state: TranscriptRenderState::default(),
            pending_clear_screen: false,
            pending_full_redraw: false,
            live_answer: String::new(),
            live_reasoning: String::new(),
            reveal_buffer: Vec::new(),
            reveal: None,
            restore_on_cancel: None,
            live_turn: None,
            ask: None,
            ask_prompt_pending: None,
            status: config.lang.choose("готов", "ready").to_string(),
            last_run,
            last_run_duration: None,
            running: false,
            dev_run: false,
            run_started_at: None,
            run_label: String::new(),
            run_token_estimate: None,
            run_activity: VecDeque::new(),
            cancel_tx: None,
            last_ctrl_c_at: None,
            footer_notice: None,
            footer_right_text: String::new(),
            footer_right_previous_text: None,
            footer_right_changed_at: None,
            should_quit: false,
            history,
            history_index: None,
            history_draft: None,
            selected_suggestion: 0,
            command_palette_opened_at: None,
            command_palette_query: String::new(),
            overlay: Overlay::None,
            chats_picker: Vec::new(),
            chats_index: 0,
            search_query: String::new(),
            search_index: 0,
            last_chat_message: None,
            pending_plan: None,
            plan_flow: PlanFlow::None,
            pending_messages: VecDeque::new(),
            effort_original: None,
            effort_focus: 0,
            settings_original: None,
            settings_focus: 0,
            effort_index: config.effort_index,
            codex_effort_index: config.codex_effort_index,
            claude_effort_index: config.claude_effort_index,
            linked_effort_split: config.linked_effort_split,
            tx,
            rx,
            usage: SessionUsage::new(),
        }
    }
}
