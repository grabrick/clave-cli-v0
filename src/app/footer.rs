use super::*;

/// Текущий ref рабочего каталога: имя ветки, а в detached HEAD — короткий SHA.
/// Не репозиторий (или ref не читается) — `None`.
///
/// Именно `symbolic-ref`, а не `rev-parse --abbrev-ref`: он честно отвечает и в свежем
/// репозитории без коммитов, где `rev-parse HEAD` падает.
pub(crate) fn detect_git_ref(dir: &Path) -> Option<String> {
    let git = |args: &[&str]| -> Option<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let value = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!value.is_empty()).then_some(value)
    };

    git(&["symbolic-ref", "--short", "HEAD"]).or_else(|| git(&["rev-parse", "--short", "HEAD"]))
}

impl App {
    /// Перечитывает индикатор git-ref. Зовём только на событиях, которые могли его
    /// поменять: старт, смена рабочего каталога, завершение агентского рана. Периодического
    /// опроса нет — спавнить `git` на простаивающем TUI незачем.
    pub(crate) fn refresh_git_ref(&mut self) {
        self.git_ref = (self.git_ref_detector)(&self.resolved_work_dir());
    }

    pub(crate) fn push_command_invocation(&mut self, command: &str) {
        self.push_system(format!("❯ {command}"));
    }

    pub(crate) fn push_command_result(&mut self, result: impl Into<String>) {
        self.push_system(format!("  ⎿  {}", result.into()));
    }

    pub(crate) fn show_footer_notice(&mut self, message: impl Into<String>) {
        self.footer_notice = Some((message.into(), Instant::now()));
    }

    pub(crate) fn expire_footer_notice(&mut self) {
        let expired = self
            .footer_notice
            .as_ref()
            .map(|(_, shown_at)| shown_at.elapsed() > Duration::from_secs(2))
            .unwrap_or(false);

        if expired {
            self.footer_notice = None;
            if self.status == self.lang.choose("подтверди выход", "confirm exit") {
                self.status = self.lang.choose("готов", "ready").to_string();
            }
        }
    }

    pub(crate) fn refresh_command_palette_state(&mut self) {
        let active = normalized_command_query(&self.input).is_some()
            && self.onboarding.is_none()
            && !self.overlay.is_open();
        if active {
            if self.command_palette_opened_at.is_none() {
                self.command_palette_opened_at = Some(Instant::now());
            }
            self.command_palette_query = self.input.clone();
        } else if self.command_palette_opened_at.is_some() {
            self.command_palette_opened_at = None;
            self.command_palette_query.clear();
        }
    }

    pub(crate) fn refresh_footer_right_state(&mut self) {
        let next = footer_right_target(self);
        if self.footer_right_text.is_empty() {
            self.footer_right_text = next;
            return;
        }

        if self.footer_right_text != next {
            self.footer_right_previous_text = Some(self.footer_right_text.clone());
            self.footer_right_text = next;
            self.footer_right_changed_at = Some(Instant::now());
            return;
        }

        let transition_done = self
            .footer_right_changed_at
            .map(|changed_at| changed_at.elapsed() > Duration::from_millis(820))
            .unwrap_or(false);
        if transition_done {
            self.footer_right_previous_text = None;
            self.footer_right_changed_at = None;
        }
    }

    pub(crate) fn handle_ctrl_c(&mut self) {
        let now = Instant::now();
        let is_double = self
            .last_ctrl_c_at
            .map(|previous| now.duration_since(previous) <= Duration::from_secs(2))
            .unwrap_or(false);
        self.last_ctrl_c_at = Some(now);

        if is_double {
            if let Some(cancel_tx) = self.cancel_tx.take() {
                let _ = cancel_tx.send(());
            }
            self.should_quit = true;
            return;
        }

        if self.running {
            if let Some(cancel_tx) = self.cancel_tx.take() {
                let _ = cancel_tx.send(());
            }
            self.status = self.lang.choose("остановка", "stopping").to_string();
            self.show_footer_notice(self.lang.choose(
                "Останавливаю выполнение. Ctrl+C ещё раз в течение 2 секунд — выйти.",
                "Stopping the run. Press Ctrl+C again within 2 seconds to exit.",
            ));
        } else {
            self.status = self
                .lang
                .choose("подтверди выход", "confirm exit")
                .to_string();
            self.show_footer_notice(self.lang.choose(
                "Нажми Ctrl+C ещё раз в течение 2 секунд, чтобы выйти.",
                "Press Ctrl+C again within 2 seconds to exit.",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static DETECTOR_CALLS: AtomicUsize = AtomicUsize::new(0);

    fn counting_detector(_dir: &Path) -> Option<String> {
        DETECTOR_CALLS.fetch_add(1, Ordering::SeqCst);
        Some("stub".to_string())
    }

    /// Временный репозиторий: коммитим с per-command config, чтобы тест не зависел от
    /// глобального git-конфига (в CI обычно нет ни user.name, ни user.email).
    fn git(dir: &Path, args: &[&str]) -> std::process::Output {
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=clave",
                "-c",
                "user.email=clave@example.com",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .expect("git")
    }

    fn temp_repo(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("clave-gitref-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    #[test]
    fn git_ref_is_the_branch_name() {
        let dir = temp_repo("branch");
        git(&dir, &["init", "-q"]);
        // Имя задаём сами: от init.defaultBranch не зависим, коммит не нужен.
        git(&dir, &["symbolic-ref", "HEAD", "refs/heads/work-branch"]);

        let got = detect_git_ref(&dir);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(got.as_deref(), Some("work-branch"));
    }

    #[test]
    fn detached_head_falls_back_to_short_sha() {
        let dir = temp_repo("detached");
        git(&dir, &["init", "-q"]);
        git(&dir, &["commit", "--allow-empty", "-q", "-m", "root"]);
        git(&dir, &["checkout", "--detach", "-q"]);
        let expected =
            String::from_utf8_lossy(&git(&dir, &["rev-parse", "--short", "HEAD"]).stdout)
                .trim()
                .to_string();

        let got = detect_git_ref(&dir);
        let _ = fs::remove_dir_all(&dir);
        assert!(!expected.is_empty());
        assert_eq!(got, Some(expected));
    }

    /// Главное свойство правки: без событий (тики цикла) детектор не дёргается повторно,
    /// а на каждое завершение рана — ровно одно чтение.
    #[test]
    fn git_ref_is_read_on_events_only() {
        let dir = temp_repo("no-poll");
        let mut app = App::new();
        app.work_dir = dir.to_string_lossy().to_string();
        app.git_ref_detector = counting_detector;
        DETECTOR_CALLS.store(0, Ordering::SeqCst);

        app.refresh_git_ref();
        assert_eq!(DETECTOR_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(app.git_ref.as_deref(), Some("stub"));

        for _ in 0..10 {
            app.drain_worker_events();
            app.refresh_footer_right_state();
        }
        assert_eq!(DETECTOR_CALLS.load(Ordering::SeqCst), 1);

        let terminal = [
            WorkerEvent::Done(0),
            WorkerEvent::ChatDone(Provider::Claude, 0, None),
            WorkerEvent::PlanReady(Provider::Claude, String::new(), 1, None),
            WorkerEvent::Cancelled,
            WorkerEvent::Failed("boom".to_string()),
        ];
        let expected = 1 + terminal.len();
        for event in terminal {
            app.tx.send(event).expect("send");
        }
        app.drain_worker_events();
        assert_eq!(DETECTOR_CALLS.load(Ordering::SeqCst), expected);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn plain_directory_has_no_git_ref() {
        let dir = temp_repo("plain");
        let got = detect_git_ref(&dir);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(got, None);
    }

    /// App без онбординга и auth-проб: `from_config` с `onboarding_done: true` не будит
    /// Onboarding и не спавнит провайдерские пробы (иначе тест зелен локально, флейкует на CI).
    /// `onboarding_done: true` ⇒ `app.onboarding == None` — это нужно palette-тестам.
    fn footer_app() -> App {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let dir = env::temp_dir().join(format!(
            "clave-footer-{}-{}",
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

    // ── ЧАСТЬ 1: push_command_invocation (35:9 →()) ─────────────────────────
    /// Вызванная команда попадает в ленту строкой «❯ {command}». Ловит `35:9 →()`
    /// (без тела ленты не пополнится и строки «❯ build» не будет).
    #[test]
    fn push_command_invocation_appends_prompt_line() {
        let mut app = footer_app();
        let before = app.transcript.len();
        app.push_command_invocation("build");
        assert!(
            app.transcript.len() > before,
            "лента не выросла после вызова команды"
        );
        assert!(
            app.transcript.iter().any(|line| line.contains("❯ build")),
            "лента не получила строку вызова команды: {:?}",
            app.transcript
        );
    }

    // ── ЧАСТЬ 2: expire_footer_notice (47:9, 50:53, 55:28) ──────────────────
    /// Свежая подсказка не гаснет. Ловит `50:53 > → <` (`0 < 2s` = true → погасило бы свежую).
    #[test]
    fn fresh_footer_notice_survives_expire() {
        let mut app = footer_app();
        app.footer_notice = Some(("hi".to_string(), Instant::now()));
        app.expire_footer_notice();
        assert!(
            app.footer_notice.is_some(),
            "свежая подсказка не должна гаснуть"
        );
    }

    /// Подсказка старше 2с гаснет. Ловит `47:9 →()` (не гасит), `50:53 > → ==`
    /// (`3s == 2s` = false → не гасит) и `50:53 > → <` (`3s < 2s` = false → не гасит).
    #[test]
    fn stale_footer_notice_expires() {
        let mut app = footer_app();
        app.footer_notice = Some(("hi".to_string(), Instant::now() - Duration::from_secs(3)));
        app.expire_footer_notice();
        assert!(
            app.footer_notice.is_none(),
            "просроченная подсказка должна погаснуть"
        );
    }

    /// При гашении просроченной подсказки статус «confirm exit» сбрасывается в «ready».
    /// Ловит `55:28 == → !=` (при равенстве `!=` ложно → статус не сбросился бы).
    #[test]
    fn expiring_notice_resets_confirm_exit_status() {
        let mut app = footer_app();
        app.status = "confirm exit".to_string();
        app.footer_notice = Some(("hi".to_string(), Instant::now() - Duration::from_secs(3)));
        app.expire_footer_notice();
        assert_eq!(
            app.status, "ready",
            "статус подтверждения выхода не сброшен"
        );
    }

    // ── ЧАСТЬ 3: refresh_command_palette_state (62:9, 63:13, 64:13, 64:16) ───
    /// Ввод-команда при закрытом overlay и без онбординга открывает палитру. Ловит
    /// `62:9 →()` (не активирует) и `64:16 delete !` (у закрытого overlay `is_open()` = false → не активирует).
    #[test]
    fn command_input_opens_palette() {
        let mut app = footer_app();
        app.input = "/help".to_string();
        app.refresh_command_palette_state();
        assert!(
            app.command_palette_opened_at.is_some(),
            "палитра не открылась на команде"
        );
        assert_eq!(app.command_palette_query, "/help");
    }

    /// Обычный текст (без '/') палитру не открывает. Ловит `63:13 && → ||`
    /// (A = false, но B && C = true → `||` дало бы active = true → открыло бы палитру).
    #[test]
    fn plain_input_keeps_palette_closed() {
        let mut app = footer_app();
        app.input = "hello".to_string();
        app.refresh_command_palette_state();
        assert!(
            app.command_palette_opened_at.is_none(),
            "палитра не должна открываться на не-команде"
        );
    }

    /// При открытом overlay палитра не открывается даже на команде. Ловит `64:13 && → ||`
    /// (!is_open = false, но A && B = true → `||` дало бы true → открыло бы палитру).
    #[test]
    fn open_overlay_blocks_palette() {
        let mut app = footer_app();
        app.input = "/help".to_string();
        app.overlay = Overlay::Search;
        app.refresh_command_palette_state();
        assert!(
            app.command_palette_opened_at.is_none(),
            "при открытом overlay палитра не должна открываться"
        );
    }

    // ── ЧАСТЬ 4: refresh_footer_right_state (77:9, 83:35, 92:52) ────────────
    /// Пустой правый текст заполняется целевым сегментом. Ловит `77:9 →()` (остался бы пустым).
    ///
    /// Не сверяем с заранее вычисленным target: слот вращается по стенным часам (`(unix/8) % N`),
    /// и на 8-секундной границе значения разошлись бы (флейк). Устойчиво: результат непуст и
    /// принадлежит набору сегментов — сам набор от фазы не зависит.
    #[test]
    fn empty_footer_right_takes_target() {
        let mut app = footer_app();
        assert!(app.footer_right_text.is_empty());
        app.refresh_footer_right_state();
        assert!(
            !app.footer_right_text.is_empty(),
            "пустой правый слот не заполнился"
        );
        assert!(
            footer_right_segments(&app).contains(&app.footer_right_text),
            "правый слот не из набора сегментов: {:?}",
            app.footer_right_text
        );
    }

    /// Смена текста сохраняет предыдущий и ставит метку времени. Ловит `83:35 != → ==`
    /// (`text != next` истинно, но `==` ложно → ветку пропустило бы, текст остался бы старым).
    ///
    /// «SENTINEL-OLD» заведомо не настоящий сегмент, поэтому `next != text` истинно при любой
    /// фазе — флейка нет.
    #[test]
    fn footer_right_change_records_previous() {
        let mut app = footer_app();
        app.footer_right_text = "SENTINEL-OLD".to_string();
        app.refresh_footer_right_state();
        assert_ne!(app.footer_right_text, "SENTINEL-OLD", "текст не обновился");
        assert_eq!(
            app.footer_right_previous_text,
            Some("SENTINEL-OLD".to_string())
        );
        assert!(app.footer_right_changed_at.is_some());
    }

    /// Просроченная (>820мс) смена очищает previous/changed_at. Ловит `92:52 > → ==`
    /// (`900 == 820` ложь → не чистит) и `92:52 > → <` (`900 < 820` ложь → не чистит).
    ///
    /// Чтобы дойти до ветки transition_done, нужен текст, равный target. Слот вращается по
    /// стенным часам, поэтому редкая 8-секундная граница между чтением target и refresh увела бы
    /// в ветку «текст сменился». Retry: берём свежий app и повторяем, пока текст остался равен
    /// target (фаза не прыгнула) — только тогда мы в нужной ветке. Без sleep и env-переменных.
    #[test]
    fn stale_footer_right_transition_clears() {
        let mut caught = false;
        for _ in 0..8 {
            let mut app = footer_app();
            let target = footer_right_target(&app);
            app.footer_right_text = target.clone();
            app.footer_right_previous_text = Some("prev".to_string());
            app.footer_right_changed_at = Some(Instant::now() - Duration::from_millis(900));
            app.refresh_footer_right_state();
            if app.footer_right_text == target {
                assert!(
                    app.footer_right_previous_text.is_none(),
                    "просроченный previous не очищен"
                );
                assert!(
                    app.footer_right_changed_at.is_none(),
                    "просроченная метка смены не очищена"
                );
                caught = true;
                break;
            }
        }
        assert!(caught, "не поймали стабильную фазу для проверки очистки");
    }
}
