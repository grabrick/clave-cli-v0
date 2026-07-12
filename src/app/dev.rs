use super::*;

/// Превращает обрамлённую строку супервайзера (`CLAVE-DEV <type> <payload>`) в строку
/// транскрипта с иконкой по типу (спека §5). Необрамлённые строки (аномалия protocol-mode
/// или сырьё stderr) возвращаются как есть — парсер не падает.
pub(crate) fn format_dev_line(raw: &str) -> String {
    let Some(rest) = raw.strip_prefix("CLAVE-DEV ") else {
        return raw.to_string();
    };
    let (type_, payload) = rest.split_once(' ').unwrap_or((rest, ""));
    let icon = match type_ {
        "progress" => "•",
        "log" => " ",
        "check" => "✓",
        "vision" => "◍",
        "diff" => "±",
        "report" => "⏺",
        "error" => "✗",
        _ => "·",
    };
    format!("{icon} {payload}")
}

/// git-корень для path (спека §4): AppleScript-независимый `git rev-parse --show-toplevel`.
fn dev_git_root(path: &Path) -> Option<PathBuf> {
    let out = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let root = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!root.is_empty()).then(|| PathBuf::from(root))
}

/// Резолв внешнего пакета clave_dev (спека §4): CLAVE_DEV_HOME → <git root>/tools/clave-dev
/// → установленный модуль. Возвращает (программа, аргументы, опциональный PYTHONPATH).
fn resolve_clave_dev(git_root: &Path) -> Option<(String, Vec<String>, Option<String>)> {
    let repo_pkg = git_root.join("tools").join("clave-dev");
    // Интерпретатор супервайзера: CLAVE_DEV_PYTHON → venv рядом с пакетом
    // (tools/clave-dev/.venv) → python3 из PATH. Системный python3 обычно без pyte, поэтому
    // venv ищем сами — иначе observer упал бы на импорте (спека §4).
    let py = env::var("CLAVE_DEV_PYTHON")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let venv = repo_pkg.join(".venv").join("bin").join("python3");
            venv.is_file().then(|| venv.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "python3".to_string());
    let module = || vec!["-m".to_string(), "clave_dev".to_string()];

    if let Ok(home) = env::var("CLAVE_DEV_HOME") {
        if !home.is_empty() {
            return Some((py, module(), Some(home)));
        }
    }
    if repo_pkg.join("clave_dev").is_dir() {
        return Some((py, module(), Some(repo_pkg.to_string_lossy().to_string())));
    }
    let importable = Command::new(&py)
        .args(["-c", "import clave_dev"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    importable.then(|| (py, module(), None))
}

/// Читатель stdout супервайзера: каждую обрамлённую строку прогоняем через
/// `format_dev_line` перед показом (спека §5). stderr читается обычным `spawn_reader`.
fn spawn_dev_reader<R: std::io::Read + Send + 'static>(reader: R, tx: Sender<WorkerEvent>) {
    use std::io::BufRead;
    thread::spawn(move || {
        let reader = std::io::BufReader::new(reader);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx.send(WorkerEvent::Line(format_dev_line(&line)));
        }
    });
}

impl App {
    /// Всё, что нужно `/dev`: git-корень репозитория и чем звать внешний clave_dev.
    /// При неудаче пишет ВНЯТНУЮ причину (какой каталог, что делать) и возвращает None.
    fn dev_target(&mut self) -> Option<(PathBuf, String, Vec<String>, Option<String>)> {
        let repo = self.resolved_work_dir();
        let Some(git_root) = dev_git_root(&repo) else {
            // Называем КАТАЛОГ и путь исправления: без этого сообщение бесполезно — чаще
            // всего clave просто запущен не из репозитория (например из домашней папки,
            // как открывается свежее окно терминала).
            self.push_system(format!(
                "✗ {} {}",
                self.lang.choose(
                    "/dev работает над git-репозиторием, а рабочий каталог им не является:",
                    "/dev needs a git repo, but the working directory is not one:",
                ),
                repo.display()
            ));
            self.push_system(self.lang.choose(
                "  ⎿ запусти clave из репозитория — или укажи его прямо здесь: /add-dir <путь>",
                "  ⎿ launch clave from the repo — or point at it right here: /add-dir <path>",
            ));
            return None;
        };
        let Some((program, args, pythonpath)) = resolve_clave_dev(&git_root) else {
            self.push_system(format!(
                "✗ {} {}",
                self.lang.choose(
                    "пакет clave_dev не найден для репозитория",
                    "clave_dev package not found for repo",
                ),
                git_root.display()
            ));
            self.push_system(self.lang.choose(
                "  ⎿ задай CLAVE_DEV_HOME, установи пакет, либо работай в репо с tools/clave-dev",
                "  ⎿ set CLAVE_DEV_HOME, install the package, or work in a repo with tools/clave-dev",
            ));
            return None;
        };
        Some((git_root, program, args, pythonpath))
    }

    /// Точка входа `/dev <задача>`: спрашивает про зрение и только потом стартует прогон.
    ///
    /// Зрение открывает окно Terminal.app прямо на экране, поэтому включать его скрытым
    /// тумблером нельзя — спрашиваем каждый раз. Если зрячего канала в системе нет вовсе
    /// (`claude` не найден), вопрос бессмысленен — идём текстом молча.
    pub(crate) fn begin_dev(&mut self, task: String) {
        if self.running {
            // busy-preflight (спека §6): второй прогон не стартуем.
            self.push_system(
                self.lang
                    .choose("Clave уже выполняется.", "Clave is already running."),
            );
            return;
        }
        self.push_system(format!("◆ /dev {task}"));

        // Сначала убеждаемся, что прогон вообще возможен, и только ПОТОМ спрашиваем про
        // зрение. Спросить, а следом сказать «так это и не запустится» — значит впустую
        // потратить ответ пользователя.
        if self.dev_target().is_none() {
            return; // причина уже показана
        }

        if !provider_binary_present("claude") {
            self.start_dev(task, false);
            return;
        }

        self.ask_prompt_pending = Some(AskPrompt {
            questions: vec![AskQuestion {
                question: self
                    .lang
                    .choose(
                        "Включить зрение для этого прогона?",
                        "Enable vision for this run?",
                    )
                    .to_string(),
                multi: false,
                // Свободный текст тут некому обработать — вопрос локальный, «да/нет».
                allow_custom: false,
                options: vec![
                    AskOption {
                        label: self
                            .lang
                            .choose("Нет — текстовое наблюдение", "No — text observation")
                            .to_string(),
                        note: Some(
                            self.lang
                                .choose("быстро, ничего не открывается", "fast, nothing pops up")
                                .to_string(),
                        ),
                    },
                    AskOption {
                        label: self.lang.choose("Да — зрение", "Yes — vision").to_string(),
                        note: Some(
                            self.lang
                                .choose(
                                    "откроется окно Terminal.app; не трогай клавиатуру и мышь",
                                    "a Terminal.app window will open; keep hands off",
                                )
                                .to_string(),
                        ),
                    },
                ],
            }],
        });
        self.ask_intent = Some(AskIntent::DevVision { task });
        self.open_pending_ask();
    }

    /// Запускает ВНЕШНИЙ clave-dev на текущем репозитории (инвариант — петля вне процесса
    /// clave). Образец — `start_task` из `runs.rs`.
    pub(crate) fn start_dev(&mut self, task: String, vision: bool) {
        if self.running {
            self.push_system(
                self.lang
                    .choose("Clave уже выполняется.", "Clave is already running."),
            );
            return;
        }

        let Some((git_root, program, mut cmd_args, pythonpath)) = self.dev_target() else {
            return; // причина уже показана
        };
        let Ok(known_good) = env::current_exe() else {
            self.push_system(self.lang.choose(
                "Не удалось определить путь текущего бинаря.",
                "Could not resolve current executable path.",
            ));
            return;
        };
        // Абсолютный канонический путь known-good (не имя из PATH), спека §3.
        let known_good = known_good.canonicalize().unwrap_or(known_good);

        let (cancel_tx, cancel_rx) = mpsc::channel();
        self.running = true;
        self.dev_run = true;
        self.run_started_at = Some(Instant::now());
        self.last_run_duration = None;
        self.run_label = "clave-dev".to_string();
        self.run_token_estimate = None;
        self.run_activity.clear();
        self.cancel_tx = Some(cancel_tx);
        self.last_ctrl_c_at = None;
        self.status = self.lang.choose("самопиление", "self-dev").to_string();

        let effort = effort_label(self.effort_index).to_string();
        let rounds = self.rounds.to_string();
        cmd_args.extend([
            task,
            "--repo".to_string(),
            git_root.to_string_lossy().to_string(),
            "--known-good".to_string(),
            known_good.to_string_lossy().to_string(),
            "--protocol".to_string(),
            "clave-dev".to_string(),
            "--effort".to_string(),
            effort,
            "--rounds".to_string(),
            rounds,
        ]);
        if vision {
            self.push_system(self.lang.choose(
                "◍ Зрение включено: на визуальном проходе откроется окно Terminal.app — не трогай в этот момент клавиатуру и мышь.",
                "◍ Vision enabled: a Terminal.app window will open during the visual pass — keep hands off the keyboard and mouse.",
            ));
            cmd_args.extend([
                "--vision".to_string(),
                "claude-cli".to_string(),
                // Гейтим ТОЛЬКО required-чеклист: эстетические мнения модели («логотип
                // тусклый») не должны блокировать автономную петлю и гонять агента
                // «чинить» несломанное.
                "--severity-threshold".to_string(),
                "none".to_string(),
            ]);
        }

        let tx = self.tx.clone();
        spawn_worker(self.tx.clone(), move || {
            let mut command = Command::new(&program);
            command
                .current_dir(&git_root)
                .args(&cmd_args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            if let Some(pp) = pythonpath {
                command.env("PYTHONPATH", pp);
            }
            configure_process_group(&mut command);
            let mut child = match command.spawn() {
                Ok(child) => child,
                Err(err) => {
                    let _ = tx.send(WorkerEvent::Failed(format!("spawn clave-dev: {err}")));
                    return;
                }
            };
            if let Some(out) = child.stdout.take() {
                spawn_dev_reader(out, tx.clone());
            }
            if let Some(err) = child.stderr.take() {
                spawn_reader(err, tx.clone()); // сырьё stderr — как Line (raw)
            }
            loop {
                if cancel_rx.try_recv().is_ok() {
                    kill_process_tree(&mut child);
                    let _ = tx.send(WorkerEvent::Cancelled);
                    return;
                }
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let _ = tx.send(WorkerEvent::Done(status.code().unwrap_or(1)));
                        return;
                    }
                    Ok(None) => thread::sleep(Duration::from_millis(80)),
                    Err(err) => {
                        let _ = tx.send(WorkerEvent::Failed(format!("wait: {err}")));
                        return;
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_formatted_by_type() {
        assert_eq!(format_dev_line("CLAVE-DEV progress раунд 1"), "• раунд 1");
        assert!(format_dev_line("CLAVE-DEV error боль").starts_with('✗'));
        assert!(format_dev_line("CLAVE-DEV report {\"converged\":true}").starts_with('⏺'));
    }

    #[test]
    fn unframed_line_passes_through() {
        assert_eq!(format_dev_line("plain cargo output"), "plain cargo output");
    }

    #[test]
    fn dev_git_root_resolves_toplevel_from_subdir() {
        let dir = env::temp_dir().join(format!("clave-dev-gr-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).expect("mkdir");
        let run = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .output()
                .expect("git");
        };
        run(&["init", "-q"]);
        let got = dev_git_root(&dir.join("sub")).and_then(|p| p.canonicalize().ok());
        let expected = dir.canonicalize().ok();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(got, expected);
    }
}
