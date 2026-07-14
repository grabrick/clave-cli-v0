use super::*;

impl App {
    pub(crate) fn suggestions(&self) -> Vec<CommandSpec> {
        let Some(needle) = normalized_command_query(&self.input) else {
            return Vec::new();
        };

        COMMANDS
            .iter()
            .copied()
            .filter(|command| {
                command.usage.starts_with(&needle) || command.insert.starts_with(&needle)
            })
            .collect()
    }

    pub(crate) fn complete_command(&mut self) {
        let suggestions = self.suggestions();
        if suggestions.is_empty() {
            return;
        }

        let index = self.selected_suggestion.min(suggestions.len() - 1);
        if let Some(suggestion) = suggestions.get(index).copied() {
            self.input = suggestion.insert.to_string();
            self.cursor = self.input.len();
        }
    }

    pub(crate) fn submit_input(&mut self) {
        let line = self.input.trim().to_string();
        self.input.clear();
        self.cursor = 0;
        self.history_index = None;

        if line.is_empty() {
            return;
        }

        self.remember_history_entry(&line);

        let normalized_plain = normalized_plain_command(&line);
        if line.eq_ignore_ascii_case("logout") || normalized_plain == "logout" {
            self.push_command_invocation(&line);
            self.push_command_result(self.lang.choose("Auth screen", "Auth screen"));
            self.open_auth_screen(
                self.lang
                    .choose(
                        "Проверь авторизацию CLI. Можно запустить Codex или Claude login.",
                        "Check CLI authentication. You can run Codex or Claude login.",
                    )
                    .to_string(),
                true,
            );
        } else if let Some(command_line) = normalize_command_line_for_execution(&line) {
            self.handle_command(&command_line);
        } else {
            self.start_chat(line);
        }
    }

    pub(crate) fn handle_command(&mut self, line: &str) {
        let mut parts = line.split_whitespace();
        let command = parts.next().unwrap_or_default();
        let rest = parts.collect::<Vec<_>>().join(" ");

        // Запускающие команды показывают запрос как «◆ …», остальные — эхо «❯ команда».
        let suppress_echo = matches!(
            command,
            "/plan"
                | "/clave"
                | "/advisor"
                | "/btw"
                | "/brainstorm"
                | "/blueprint"
                | "/finish-branch"
                | "/split-work"
                | "/worktrees"
                | "/autofix-pr"
                | "/new"
                | "/resume"
                | "/quit"
                | "/exit"
        );
        if !suppress_echo {
            self.push_command_invocation(line);
        }

        match command {
            "/help" => {
                self.push_system(self.lang.choose("⏺ Команды", "⏺ Commands"));
                for command in COMMANDS {
                    self.push_system(format!(
                        "  ⎿ {:<22} {}",
                        command.usage,
                        command.description(self.lang)
                    ));
                }
                self.status = self.lang.choose("помощь", "help").to_string();
            }
            "/lang" | "/language" => match rest.as_str() {
                "ru" | "рус" | "russian" => {
                    self.lang = Language::Ru;
                    self.status = "язык:ru".to_string();
                    self.save_current_config(true);
                    self.push_command_result("Язык интерфейса изменён на русский.");
                }
                "en" | "eng" | "english" => {
                    self.lang = Language::En;
                    self.status = "lang:en".to_string();
                    self.save_current_config(true);
                    self.push_command_result("Interface language changed to English.");
                }
                _ => self.push_system(
                    self.lang
                        .choose("Использование: /lang ru|en", "Usage: /lang ru|en"),
                ),
            },
            "/mode" => match rest.as_str() {
                "codex-only" => self.apply_mode(Mode::CodexOnly),
                "claude-only" => self.apply_mode(Mode::ClaudeOnly),
                "claude-codex" => self.apply_mode(Mode::ClaudeCodex),
                "codex-claude" => self.apply_mode(Mode::CodexClaude),
                _ => self.push_system(self.lang.choose(
                    "Использование: /mode codex-only|claude-only|claude-codex|codex-claude",
                    "Usage: /mode codex-only|claude-only|claude-codex|codex-claude",
                )),
            },
            "/settings" => self.open_settings(),
            "/chat-model" => match Provider::from_str(rest.trim()) {
                Some(provider) => self.set_direct_provider(provider),
                None => self.push_system(self.lang.choose(
                    "Использование: /chat-model codex|claude",
                    "Usage: /chat-model codex|claude",
                )),
            },
            "/theme" => match Theme::from_str(rest.trim()) {
                Some(theme) => self.set_theme(theme),
                None => self.push_system(self.lang.choose(
                    "Использование: /theme purple|cyan|rose|amber|mono",
                    "Usage: /theme purple|cyan|rose|amber|mono",
                )),
            },
            "/roles" => {
                let providers = rest
                    .split(|ch: char| ch.is_whitespace() || matches!(ch, '>' | '-' | '→'))
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>();
                match providers.as_slice() {
                    [architect, reviewer] => {
                        match (Provider::from_str(architect), Provider::from_str(reviewer)) {
                            (Some(architect), Some(reviewer)) => {
                                self.set_roles(architect, reviewer);
                            }
                            _ => self.push_system(self.lang.choose(
                                "Использование: /roles codex|claude codex|claude",
                                "Usage: /roles codex|claude codex|claude",
                            )),
                        }
                    }
                    _ => self.push_system(self.lang.choose(
                        "Использование: /roles <исполнитель> <ревьюер>",
                        "Usage: /roles <executor> <reviewer>",
                    )),
                }
            }
            "/brainstorm" => self.run_planning_preset(
                "Брейншторминг перед реализацией",
                "Brainstorm before implementation",
                &rest,
                "Разбери текущий контекст, предложи варианты решения, риски, быстрые проверки и лучший следующий шаг.",
                "Use the current context, propose solution options, risks, quick checks, and the best next step.",
            ),
            "/blueprint" => self.run_planning_preset(
                "План разработки",
                "Development plan",
                &rest,
                "Собери из текущего контекста пошаговый план реализации с проверками и порядком изменений.",
                "Turn the current context into a step-by-step implementation plan with checks and change order.",
            ),
            "/finish-branch" => self.run_planning_preset(
                "Завершение ветки разработки",
                "Finish development branch",
                &rest,
                "Проверь, что нужно доделать перед завершением ветки: тесты, регрессии, документация, пуш.",
                "Check what is needed before finishing the branch: tests, regressions, docs, and push readiness.",
            ),
            "/split-work" => self.run_planning_preset(
                "Разделение работы между агентами",
                "Split work across agents",
                &rest,
                "Разбей текущую задачу на независимые рабочие потоки для нескольких ИИ-агентов.",
                "Split the current task into independent workstreams for multiple AI agents.",
            ),
            "/worktrees" => self.run_planning_preset(
                "План работы через git worktrees",
                "Git worktree workflow plan",
                &rest,
                "Предложи безопасную схему работы через git worktrees для параллельной разработки.",
                "Propose a safe git worktree workflow for parallel development.",
            ),
            "/advisor" => self.run_advisor_command(&rest),
            "/btw" => self.run_btw_command(&rest),
            "/autofix-pr" => self.run_planning_preset(
                "Autofix PR",
                "Autofix PR",
                &rest,
                "Проанализируй текущую ветку как PR: найди вероятные проблемы, недостающие проверки и план исправлений.",
                "Analyze the current branch as a PR: find likely issues, missing checks, and a fix plan.",
            ),
            "/agents" => self.open_settings_from(),
            "/background" => {
                self.status = self.lang.choose("сессия сохранена", "session saved").to_string();
                self.push_command_result(self.lang.choose(
                    "Чат уже сохраняется на диск. Используй /quit, чтобы закрыть UI.",
                    "This chat is already saved on disk. Use /quit to close the UI.",
                ));
            }
            "/branch" => self.branch_current_chat(),
            "/add-dir" => self.set_work_dir_command(&rest),
            "/color" => match Theme::from_str(rest.trim()) {
                Some(theme) => self.set_theme(theme),
                None => self.push_system(self.lang.choose(
                    "Использование: /color purple|cyan|rose|amber|mono",
                    "Usage: /color purple|cyan|rose|amber|mono",
                )),
            },
            "/plan" | "/clave" => {
                if rest.trim().is_empty() {
                    self.push_system(
                        self.lang
                            .choose("Использование: /plan <задача>", "Usage: /plan <task>"),
                    );
                } else {
                    self.start_task(rest.trim().to_string());
                }
            }
            "/rounds" => match rest.parse::<usize>() {
                Ok(value) if value > 0 => {
                    self.rounds = value;
                    self.status = format!("rounds:{value}");
                    self.save_current_config(true);
                    self.push_command_result(format!(
                        "{} {value}.",
                        self.lang.choose("Количество раундов:", "Rounds set to")
                    ));
                }
                _ => self.push_system(self.lang.choose(
                    "Использование: /rounds <положительное-число>",
                    "Usage: /rounds <positive-number>",
                )),
            },
            "/out" => {
                if rest.trim().is_empty() {
                    self.push_system(
                        self.lang
                            .choose("Использование: /out <папка>", "Usage: /out <directory>"),
                    );
                } else {
                    self.out_dir = rest;
                    self.status = self
                        .lang
                        .choose("папка обновлена", "out updated")
                        .to_string();
                    self.save_current_config(true);
                    self.push_command_result(format!(
                        "{} {}.",
                        self.lang.choose("Папка артефактов:", "Output directory:"),
                        self.out_dir
                    ));
                }
            }
            "/status" => self.show_status(),
            "/cost" => self.show_cost(),
            "/version" => self.show_version(),
            "/uptime" => self.show_uptime(),
            "/retry" => self.retry_last(),
            "/export" => self.export_chat(),
            "/search" => self.open_search(),
            "/effort" => {
                self.effort_original = Some(self.effort_snapshot());
                self.effort_focus = 0;
                self.overlay = Overlay::Effort;
                self.status = "effort".to_string();
            }
            "/logout" | "/auth" => {
                self.push_command_result(self.lang.choose("Auth screen", "Auth screen"));
                self.open_auth_screen(
                    self.lang
                        .choose(
                            "Проверь авторизацию CLI. Можно запустить Codex или Claude login.",
                            "Check CLI authentication. You can run Codex or Claude login.",
                        )
                        .to_string(),
                    true,
                );
            }
            "/setup" => {
                self.onboarding = Some(Onboarding::new(self.mode));
                self.status = self.lang.choose("настройка", "setup").to_string();
            }
            "/new" => self.start_new_chat(),
            "/name" | "/rename" => self.rename_current_chat(&rest),
            "/chats" => {
                if rest.trim() == "clear" {
                    self.clear_small_chats();
                } else {
                    self.open_chats_picker();
                }
            }
            "/resume" => {
                if rest.trim().is_empty() {
                    self.open_chats_picker();
                } else {
                    self.resume_chat(rest.trim());
                }
            }
            "/clear" => {
                if rest.trim() == "history" {
                    self.clear_all_chats();
                } else {
                    // Как /clear в Claude: контекст И текущий именованный чат уходят.
                    self.clear_current_chat();
                }
            }
            "/quit" | "/exit" => self.should_quit = true,
            _ => self.push_system(format!(
                "{} {command}",
                self.lang.choose("Неизвестная команда:", "Unknown command:")
            )),
        }
    }

    pub(crate) fn apply_mode(&mut self, mode: Mode) {
        self.set_mode(mode);
        self.status = format!("mode:{}", self.mode.as_str());
        self.save_current_config(true);
        self.push_command_result(format!(
            "{} {}.",
            self.lang.choose("Режим изменён на", "Mode changed to"),
            self.mode.as_str()
        ));
        self.ensure_auth_ready_for_current_mode();
    }

    fn show_status(&mut self) {
        self.status = self.lang.choose("статус", "status").to_string();
        self.push_system(self.lang.choose("⏺ Статус сессии", "⏺ Session status"));
        self.push_status_row(
            self.lang.choose("Режим", "Mode"),
            self.mode.as_str().to_string(),
        );
        self.push_status_row(
            self.lang.choose("Исполнитель", "Executor"),
            self.mode.architect_provider().title().to_string(),
        );
        self.push_status_row(
            self.lang.choose("Ревьюер", "Reviewer"),
            self.mode.reviewer_provider().title().to_string(),
        );
        self.push_status_row(
            self.lang.choose("Простой чат", "Direct chat"),
            self.direct_provider.title().to_string(),
        );
        self.push_status_row(self.lang.choose("Effort", "Effort"), self.effort_summary());
        self.push_status_row(
            self.lang.choose("Раунды", "Rounds"),
            self.rounds.to_string(),
        );
        self.push_status_row(
            self.lang.choose("Язык", "Language"),
            self.lang.as_str().to_string(),
        );
        self.push_status_row(
            self.lang.choose("Тема", "Theme"),
            self.theme.title().to_string(),
        );
        self.push_status_row(
            self.lang.choose("Рабочая директория", "Working directory"),
            self.resolved_work_dir().display().to_string(),
        );
        self.push_status_row(
            self.lang.choose("Артефакты", "Artifacts"),
            self.out_dir.clone(),
        );
        self.push_status_row(self.lang.choose("Чат", "Chat"), self.chat_id.clone());
    }

    fn show_version(&mut self) {
        let mark = |present: bool| if present { "✓" } else { "✗" };
        self.push_system(format!("⏺ {APP_COMMAND} v{}", env!("CARGO_PKG_VERSION")));
        self.push_system(format!(
            "  {} · {}",
            env!("CARGO_PKG_REPOSITORY"),
            env!("CARGO_PKG_LICENSE")
        ));
        self.push_system(format!(
            "  claude {}  codex {}",
            mark(provider_binary_present("claude")),
            mark(provider_binary_present("codex")),
        ));
        self.push_system(format!(
            "  {}: {}",
            self.lang.choose("режим", "mode"),
            self.mode.as_str()
        ));
        self.push_system(format!(
            "  {}: {}",
            self.lang.choose("состояние", "state"),
            clave_state_dir().display()
        ));
        self.status = self.lang.choose("версия", "version").to_string();
    }

    fn show_cost(&mut self) {
        self.status = self.lang.choose("расход", "cost").to_string();
        self.push_system(self.lang.choose("⏺ Расход сессии", "⏺ Session cost"));

        let claude = self.usage.claude;
        let codex = self.usage.codex;
        let total_tokens = self.usage.total_tokens();
        let total_cost = self.usage.total_cost_usd();
        let minutes = self.usage.started_at.elapsed().as_secs() / 60;

        if claude.requests == 0 && codex.requests == 0 {
            self.push_status_row(
                self.lang.choose("Данные", "Data"),
                self.lang
                    .choose("пока нет запросов", "no requests yet")
                    .to_string(),
            );
            return;
        }

        let req = self.lang.choose("запр.", "req");
        if claude.requests > 0 {
            self.push_status_row(
                "Claude",
                format!(
                    "{} {req} · {} in · {} out · ${:.4}",
                    claude.requests,
                    format_token_count(claude.total.input as usize),
                    format_token_count(claude.total.output as usize),
                    claude.total.cost_usd,
                ),
            );
        }
        if codex.requests > 0 {
            self.push_status_row(
                "Codex",
                format!(
                    "{} {req} · {} tok · $—",
                    codex.requests,
                    format_token_count(codex.total.tokens() as usize),
                ),
            );
        }
        self.push_status_row(
            self.lang.choose("Итого", "Total"),
            format!(
                "≈ {} {} · ${:.4}",
                format_token_count(total_tokens as usize),
                self.lang.choose("токенов", "tokens"),
                total_cost,
            ),
        );
        self.push_status_row(
            self.lang.choose("Сессия", "Session"),
            format!(
                "{minutes} {} · {}",
                self.lang.choose("мин", "min"),
                self.lang.choose(
                    "read-only chat, инструменты отключены",
                    "read-only chat, tools disabled"
                ),
            ),
        );
    }

    fn show_uptime(&mut self) {
        self.status = self.lang.choose("аптайм", "uptime").to_string();
        self.push_system(
            self.lang
                .choose("⏺ Время работы сессии", "⏺ Session uptime"),
        );
        self.push_status_row(
            self.lang.choose("Работает", "Running"),
            format_elapsed(self.usage.started_at.elapsed()),
        );
    }

    fn retry_last(&mut self) {
        match self.last_chat_message.clone() {
            Some(message) => self.start_chat(message),
            None => self.push_system(self.lang.choose(
                "Нет последнего запроса для повтора.",
                "No previous request to retry.",
            )),
        }
    }

    fn export_chat(&mut self) {
        let dir = self.resolved_work_dir();
        let path = dir.join(format!("clave-{}.md", sanitize_chat_id(&self.chat_id)));
        let content = format!(
            "# Clave · {}\n\n{}\n",
            self.chat_id,
            self.transcript.join("\n")
        );
        match fs::write(&path, content) {
            Ok(()) => self.push_system(format!(
                "{} {}",
                self.lang.choose("Чат экспортирован:", "Chat exported:"),
                path.display()
            )),
            Err(err) => self.push_system(format!(
                "{} {}",
                self.lang
                    .choose("Не удалось экспортировать:", "Export failed:"),
                err
            )),
        }
    }

    fn push_status_row(&mut self, label: &str, value: String) {
        self.push_system(format!("  ⎿ {label}: {value}"));
    }

    #[cfg(test)]
    pub(crate) fn command_has_handler(command: &str) -> bool {
        matches!(
            command,
            "/brainstorm"
                | "/blueprint"
                | "/finish-branch"
                | "/split-work"
                | "/worktrees"
                | "/add-dir"
                | "/advisor"
                | "/agents"
                | "/autofix-pr"
                | "/background"
                | "/branch"
                | "/btw"
                | "/plan"
                | "/clear"
                | "/new"
                | "/chats"
                | "/resume"
                | "/color"
                | "/effort"
                | "/settings"
                | "/chat-model"
                | "/theme"
                | "/roles"
                | "/logout"
                | "/help"
                | "/lang"
                | "/mode"
                | "/rounds"
                | "/out"
                | "/status"
                | "/cost"
                | "/version"
                | "/uptime"
                | "/retry"
                | "/export"
                | "/search"
                | "/name"
                | "/rename"
                | "/setup"
                | "/quit"
        )
    }

    fn run_planning_preset(
        &mut self,
        title_ru: &'static str,
        title_en: &'static str,
        rest: &str,
        fallback_ru: &'static str,
        fallback_en: &'static str,
    ) {
        let focus = if rest.trim().is_empty() {
            self.lang.choose(fallback_ru, fallback_en).to_string()
        } else {
            rest.trim().to_string()
        };
        let task = format!("{}:\n{}", self.lang.choose(title_ru, title_en), focus);
        self.start_task(task);
    }

    fn run_advisor_command(&mut self, rest: &str) {
        let prompt = if rest.trim().is_empty() {
            self.lang
                .choose(
                    "Оцени текущий контекст как технический советник: что я упускаю, какой следующий шаг самый разумный, какие риски проверить?",
                    "Review the current context as a technical advisor: what am I missing, what is the smartest next step, and which risks should be checked?",
                )
                .to_string()
        } else {
            format!(
                "{}\n{}",
                self.lang.choose(
                    "Ответь как технический советник. Дай ясную рекомендацию без запуска planning-loop:",
                    "Answer as a technical advisor. Give a clear recommendation without running the planning loop:",
                ),
                rest.trim()
            )
        };
        let display = if rest.trim().is_empty() {
            "/advisor".to_string()
        } else {
            format!("/advisor {}", rest.trim())
        };
        self.start_chat_with_prompt(display, prompt);
    }

    fn run_btw_command(&mut self, rest: &str) {
        if rest.trim().is_empty() {
            self.push_system(
                self.lang
                    .choose("Использование: /btw <вопрос>", "Usage: /btw <question>"),
            );
            return;
        }

        let prompt = format!(
            "{}\n{}",
            self.lang.choose(
                "Ответь на быстрый побочный вопрос, не меняя план и не трогая файлы:",
                "Answer this quick side question without changing the plan or touching files:",
            ),
            rest.trim()
        );
        self.start_chat_with_prompt(format!("/btw {}", rest.trim()), prompt);
    }

    fn branch_current_chat(&mut self) {
        let source_id = self.chat_id.clone();
        let transcript = self.transcript.clone();
        self.chat_id = new_chat_id();
        self.chat_path = chat_path_for_id(&self.chats_dir, &self.chat_id);
        self.transcript = transcript;
        self.chat_title_custom = false;
        self.chat_title = chat_display_title(&self.chat_path, &self.transcript, &self.chat_id);
        self.last_run = find_last_run(&self.transcript);

        match save_chat_transcript(&self.chat_path, &self.chat_id, &self.transcript) {
            Ok(()) => {
                self.status = self
                    .lang
                    .choose("ветка создана", "branch created")
                    .to_string();
                self.save_current_config(true);
                self.push_command_result(format!(
                    "{} {} → {}",
                    self.lang
                        .choose("Создана ветка чата:", "Chat branch created:"),
                    source_id,
                    self.chat_id
                ));
            }
            Err(err) => self.push_command_result(format!(
                "{} {}",
                self.lang
                    .choose("Не удалось создать ветку:", "Failed to create branch:"),
                err
            )),
        }
    }

    fn set_work_dir_command(&mut self, rest: &str) {
        let value = rest.trim();
        if value.is_empty() {
            self.push_command_result(self.lang.choose(
                "Использование: /add-dir <папка>",
                "Usage: /add-dir <directory>",
            ));
            return;
        }

        let candidate = PathBuf::from(value);
        let base_dir = launch_work_dir();
        let resolved = if candidate.is_absolute() {
            candidate
        } else {
            base_dir.join(candidate)
        };

        if !resolved.is_dir() {
            self.push_command_result(format!(
                "{} {}",
                self.lang
                    .choose("Папка не найдена:", "Directory does not exist:"),
                resolved.display()
            ));
            return;
        }

        self.work_dir = resolved.to_string_lossy().to_string();
        // Индикатор футера — про НОВЫЙ каталог, и уже в этом кадре.
        self.refresh_git_ref();
        self.status = self.lang.choose("cwd обновлён", "cwd updated").to_string();
        self.save_current_config(true);
        self.push_command_result(format!(
            "{} {}",
            self.lang
                .choose("Рабочая директория:", "Working directory:"),
            self.work_dir
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_palette_command_has_a_handler() {
        for command in COMMANDS {
            assert!(
                App::command_has_handler(command.command_token()),
                "missing handler for {}",
                command.command_token()
            );
        }
    }

    fn stub_git_ref(_dir: &std::path::Path) -> Option<String> {
        Some("stub".to_string())
    }

    /// Каталог уникален на процесс И на вызов: иначе параллельные прогоны затирают
    /// файлы друг друга и тест начинает падать вразброс.
    fn temp_commands_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = env::temp_dir().join(format!(
            "clave-cmd-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    /// App на временных путях. `App::new()` брать нельзя: она читает настоящий конфиг
    /// и при непройденном онбординге поднимает auth-probe процессы провайдеров.
    ///
    /// `running = true` — намеренно: запускающие команды (`/plan`, `/advisor`, …)
    /// на busy-преflight отвечают сообщением и НЕ поднимают CLI провайдера.
    fn app_for_commands() -> (App, PathBuf) {
        let dir = temp_commands_dir();
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
        app.lang = Language::Ru;
        app.onboarding = None;
        app.overlay = Overlay::None;
        app.git_ref_detector = stub_git_ref;
        app.work_dir = dir.to_string_lossy().to_string();
        app.running = true;
        (app, dir)
    }

    /// Строки, которые команда добавила в ленту (эхо + результат). Команды вроде `/new`
    /// и `/resume` ленту сбрасывают — тогда новой считается вся лента целиком.
    fn run(app: &mut App, line: &str) -> Vec<String> {
        let before = app.transcript.len();
        app.handle_command(line);
        let from = if app.transcript.len() < before {
            0
        } else {
            before
        };
        app.transcript[from..].to_vec()
    }

    fn joined(app: &mut App, line: &str) -> String {
        run(app, line).join("\n")
    }

    const BUSY: &str = "Clave уже выполняется.";

    /// Эхо «❯ команда» печатается для обычных команд и подавляется для запускающих:
    /// у тех свой заголовок «◆ …», два заголовка подряд выглядели бы как дубль.
    #[test]
    fn command_echo_is_suppressed_only_for_launching_commands() {
        let (mut app, dir) = app_for_commands();

        let help = run(&mut app, "/help");
        assert_eq!(help[0], "❯ /help");

        let plan = run(&mut app, "/plan задача");
        assert!(
            !plan[0].starts_with("❯ "),
            "у запускающей команды эха быть не должно: {plan:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// Каждая ветка диспетчера обязана оставлять свой отличимый след: если ветку
    /// выкинуть, команда провалится в «Неизвестная команда».
    #[test]
    fn handle_command_dispatches_every_branch() {
        let (mut app, dir) = app_for_commands();

        assert!(joined(&mut app, "/help").contains("⏺ Команды"));
        assert_eq!(app.status, "помощь");

        assert!(joined(&mut app, "/lang en").contains("Interface language changed to English."));
        assert_eq!(app.lang, Language::En);
        assert!(joined(&mut app, "/language ru").contains("Язык интерфейса изменён на русский."));
        assert_eq!(app.lang, Language::Ru);
        assert!(joined(&mut app, "/lang xx").contains("Использование: /lang ru|en"));
        assert_eq!(app.lang, Language::Ru);

        // Валидные /mode и /roles тут не гоняем: они уходят в auth-probe (спавн CLI).
        assert!(joined(&mut app, "/mode bogus").contains("Использование: /mode codex-only"));
        assert!(joined(&mut app, "/roles codex").contains("Использование: /roles <исполнитель>"));
        assert!(
            joined(&mut app, "/roles alpha beta")
                .contains("Использование: /roles codex|claude codex|claude"),
            "два аргумента разбираются как пара ролей, даже если провайдеры неизвестны"
        );

        app.overlay = Overlay::None;
        assert!(!joined(&mut app, "/settings").contains("Неизвестная"));
        assert_eq!(app.overlay, Overlay::Settings);
        app.overlay = Overlay::None;
        assert!(!joined(&mut app, "/agents").contains("Неизвестная"));
        assert_eq!(app.overlay, Overlay::Settings);
        app.overlay = Overlay::None;
        assert!(!joined(&mut app, "/search").contains("Неизвестная"));
        assert_eq!(app.overlay, Overlay::Search);
        app.overlay = Overlay::None;
        assert!(!joined(&mut app, "/effort").contains("Неизвестная"));
        assert_eq!(app.overlay, Overlay::Effort);
        assert_eq!(app.status, "effort");
        app.overlay = Overlay::None;

        // /logout, /auth и /setup здесь не гоняем: обе ветки строят Onboarding::new,
        // а он безусловно поднимает auth-probe процессы codex и claude. В юнит-тесте
        // это живой спавн провайдера — запрещено. Мутанты на этих match-arm остаются
        // непокрытыми осознанно; закрыть их можно только вынеся probe за App.

        assert!(joined(&mut app, "/chat-model claude").contains("Модель для простых сообщений:"));
        assert_eq!(app.direct_provider, Provider::Claude);
        assert!(
            joined(&mut app, "/chat-model x").contains("Использование: /chat-model codex|claude")
        );

        assert!(joined(&mut app, "/theme cyan").contains("Цветовая гамма:"));
        assert_eq!(app.theme, Theme::Cyan);
        assert!(joined(&mut app, "/theme x").contains("Использование: /theme purple"));
        assert!(joined(&mut app, "/color rose").contains("Цветовая гамма:"));
        assert_eq!(app.theme, Theme::Rose);
        assert!(joined(&mut app, "/color x").contains("Использование: /color purple"));

        assert!(joined(&mut app, "/background").contains("Чат уже сохраняется на диск."));
        assert_eq!(app.status, "сессия сохранена");

        assert!(joined(&mut app, &format!("/add-dir {}", dir.display()))
            .contains("Рабочая директория:"));

        assert!(joined(&mut app, "/rounds 3").contains("Количество раундов: 3."));
        assert_eq!(app.rounds, 3);
        assert!(joined(&mut app, "/rounds 0").contains("Использование: /rounds"));
        assert_eq!(app.rounds, 3, "ноль раундов — не режим работы, а поломка");
        assert!(joined(&mut app, "/rounds -1").contains("Использование: /rounds"));
        assert_eq!(app.rounds, 3);

        assert!(joined(&mut app, "/out artifacts").contains("Папка артефактов: artifacts."));
        assert_eq!(app.out_dir, "artifacts");
        assert!(joined(&mut app, "/out").contains("Использование: /out <папка>"));

        assert!(joined(&mut app, "/quit").is_empty());
        assert!(app.should_quit);
        app.should_quit = false;
        assert!(joined(&mut app, "/exit").is_empty());
        assert!(app.should_quit);

        assert!(joined(&mut app, "/bogus").contains("Неизвестная команда: /bogus"));

        let _ = fs::remove_dir_all(&dir);
    }

    /// Запускающие команды при занятом приложении отвечают busy-преflight, а не
    /// «неизвестная команда»: так каждая ветка проверяется без спавна провайдера.
    #[test]
    fn launching_commands_reach_their_runners() {
        let (mut app, dir) = app_for_commands();

        for command in [
            "/plan задача",
            "/clave задача",
            "/brainstorm",
            "/blueprint",
            "/finish-branch",
            "/split-work",
            "/worktrees",
            "/autofix-pr",
            "/advisor",
            "/advisor как быть",
            "/btw вопрос",
        ] {
            let out = joined(&mut app, command);
            assert!(
                out.contains(BUSY),
                "{command} обязана дойти до запуска (busy-преflight), получено: {out}"
            );
        }

        assert!(joined(&mut app, "/plan").contains("Использование: /plan <задача>"));
        assert!(joined(&mut app, "/btw").contains("Использование: /btw <вопрос>"));

        assert!(joined(&mut app, "/retry").contains("Нет последнего запроса для повтора."));
        app.last_chat_message = Some("прошлый запрос".to_string());
        assert!(
            joined(&mut app, "/retry").contains("в очереди: прошлый запрос"),
            "повтор обязан отправить прошлое сообщение"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// Чатовые ветки диспетчера: аргумент решает, что именно произойдёт с файлами.
    #[test]
    fn chat_commands_route_by_argument() {
        let (mut app, dir) = app_for_commands();
        let other = chat_path_for_id(&dir, "chat-other");
        save_chat_transcript(&other, "chat-other", &["строка".to_string()]).expect("save");

        assert!(joined(&mut app, "/name Мой чат").contains("Чат назван: Мой чат"));
        assert!(joined(&mut app, "/rename Другой").contains("Чат назван: Другой"));

        // /chats открывает пикер, /chats clear — чистит мелочь.
        app.overlay = Overlay::None;
        run(&mut app, "/chats");
        assert_eq!(app.overlay, Overlay::Chats);
        app.overlay = Overlay::None;
        assert!(joined(&mut app, "/chats clear").contains("Удалено мелких чатов: 1"));
        assert!(!other.exists());
        assert_eq!(
            app.overlay,
            Overlay::None,
            "/chats clear не открывает пикер"
        );

        // /resume без аргумента — тот же пикер, с аргументом — открытие чата.
        let full = chat_path_for_id(&dir, "chat-full");
        save_chat_transcript(&full, "chat-full", &["a".to_string(), "b".to_string()])
            .expect("save");
        run(&mut app, "/resume");
        assert_eq!(app.overlay, Overlay::Chats);
        app.overlay = Overlay::None;
        assert!(joined(&mut app, "/resume chat-full").contains("Чат открыт: chat-full"));
        assert_eq!(app.chat_id, "chat-full");

        // /branch — копия чата под новым id, исходный на месте.
        let source = app.chat_path.clone();
        assert!(joined(&mut app, "/branch").contains("Создана ветка чата: chat-full →"));
        assert_ne!(app.chat_id, "chat-full");
        assert!(source.exists() && app.chat_path.exists());
        assert_eq!(app.transcript.iter().filter(|l| *l == "a").count(), 1);

        // /new — свежий чат, старый остаётся; /clear — текущий чат уходит с диска.
        let before_new = app.chat_path.clone();
        assert!(joined(&mut app, "/new").contains("Новый чат:"));
        assert!(before_new.exists());
        let cleared = app.chat_path.clone();
        run(&mut app, "/clear");
        assert!(!cleared.exists(), "/clear удаляет текущий чат");
        assert!(full.exists(), "/clear не трогает остальную историю");

        // /clear history — наоборот: сносит всю прошлую историю, щадит текущий чат.
        let current = app.chat_path.clone();
        assert!(joined(&mut app, "/clear history").contains("Удалено чатов:"));
        assert!(!full.exists(), "/clear history обязан стереть старые чаты");
        assert!(current.exists(), "/clear history не трогает текущий чат");

        let _ = fs::remove_dir_all(&dir);
    }

    /// /status, /version, /uptime и /export печатают конкретные строки — молчаливая
    /// заглушка вместо них означала бы «команда есть, а ответа нет».
    #[test]
    fn info_commands_report_session_facts() {
        let (mut app, dir) = app_for_commands();

        let status = joined(&mut app, "/status");
        assert!(status.contains("⏺ Статус сессии"));
        assert!(status.contains(&format!("  ⎿ Режим: {}", app.mode.as_str())));
        assert!(status.contains(&format!("  ⎿ Раунды: {}", app.rounds)));
        assert!(status.contains(&format!("  ⎿ Чат: {}", app.chat_id)));
        assert_eq!(app.status, "статус");

        let version = joined(&mut app, "/version");
        assert!(version.contains(&format!("⏺ {APP_COMMAND} v{}", env!("CARGO_PKG_VERSION"))));
        assert!(version.contains("claude "));
        assert_eq!(app.status, "версия");

        let uptime = joined(&mut app, "/uptime");
        assert!(uptime.contains("⏺ Время работы сессии"));
        assert!(uptime.contains("  ⎿ Работает: "));
        assert_eq!(app.status, "аптайм");

        let export = joined(&mut app, "/export");
        assert!(export.contains("Чат экспортирован:"));
        let exported = dir.join(format!("clave-{}.md", app.chat_id));
        let content = fs::read_to_string(&exported).expect("экспорт обязан лечь на диск");
        assert!(content.contains(&app.chat_id));

        let _ = fs::remove_dir_all(&dir);
    }

    /// /cost: без запросов — честное «пока нет запросов»; с запросами — только те
    /// провайдеры, что реально работали, и минуты сессии (а не секунды и не остаток).
    #[test]
    fn cost_reports_only_providers_that_ran() {
        let (mut app, dir) = app_for_commands();

        let empty = joined(&mut app, "/cost");
        assert!(empty.contains("⏺ Расход сессии"));
        assert!(empty.contains("  ⎿ Данные: пока нет запросов"));
        assert!(!empty.contains("Claude:") && !empty.contains("Codex:"));
        assert_eq!(app.status, "расход");

        app.usage.started_at = Instant::now() - Duration::from_secs(300);
        app.usage.claude.requests = 2;
        app.usage.claude.total = RunUsage {
            input: 1000,
            output: 500,
            cost_usd: 0.25,
            ..RunUsage::default()
        };
        let claude_only = joined(&mut app, "/cost");
        assert!(claude_only.contains("  ⎿ Claude: 2 запр."));
        assert!(claude_only.contains("$0.2500"));
        assert!(
            !claude_only.contains("  ⎿ Codex:"),
            "Codex без запросов показывать нельзя: {claude_only}"
        );
        assert!(claude_only.contains("  ⎿ Итого: "));
        assert!(
            claude_only.contains("  ⎿ Сессия: 5 мин"),
            "минуты сессии считаются как секунды/60: {claude_only}"
        );
        assert!(!claude_only.contains("пока нет запросов"));

        let (mut app, _) = app_for_commands();
        app.usage.codex.requests = 3;
        app.usage.codex.total = RunUsage {
            input: 700,
            output: 300,
            ..RunUsage::default()
        };
        let codex_only = joined(&mut app, "/cost");
        assert!(codex_only.contains("  ⎿ Codex: 3 запр."));
        assert!(
            !codex_only.contains("  ⎿ Claude:"),
            "Claude без запросов показывать нельзя: {codex_only}"
        );
        assert!(!codex_only.contains("пока нет запросов"));

        let _ = fs::remove_dir_all(&dir);
    }

    /// Tab-дополнение подставляет команду, даже если курсор подсказок «уехал» дальше
    /// их числа: индекс обязан прижиматься к последней подсказке.
    #[test]
    fn complete_command_clamps_the_selection_to_the_last_suggestion() {
        let (mut app, dir) = app_for_commands();
        app.input = "/uptim".to_string();
        assert_eq!(app.suggestions().len(), 1);
        app.selected_suggestion = 5;

        app.complete_command();

        assert_eq!(app.input, "/uptime");
        assert_eq!(app.cursor, app.input.len());

        let _ = fs::remove_dir_all(&dir);
    }

    /// Смена рабочего каталога обязана обновить индикатор футера в ЭТОМ же кадре: иначе
    /// футер продолжал бы показывать ветку прошлого каталога.
    #[test]
    fn set_work_dir_moves_the_cwd_and_refreshes_the_git_indicator() {
        let (mut app, dir) = app_for_commands();
        app.git_ref = None;

        app.set_work_dir_command(&dir.to_string_lossy());
        assert_eq!(app.work_dir, dir.to_string_lossy());
        assert_eq!(app.git_ref.as_deref(), Some("stub"));

        // Несуществующая папка: ни каталог, ни индикатор не трогаем.
        app.git_ref = None;
        app.set_work_dir_command(&dir.join("missing").to_string_lossy());
        assert_eq!(app.work_dir, dir.to_string_lossy());
        assert_eq!(app.git_ref, None);

        let _ = fs::remove_dir_all(&dir);
    }

    // ─────────────────────────── /mode и /roles ───────────────────────────
    //
    // Их не покрыли, решив, что они «уходят в auth-probe (спавн CLI)». Это не так: путь
    // `/mode` → `apply_mode` → `set_mode` только присваивает поля, а `save_current_config`
    // пишет во временный конфиг теста. Живых проб на нём нет — проверено. Освобождение от
    // проверки всегда обосновано и всегда выходит боком: тут за ним прятались ШЕСТЬ мутантов,
    // включая «apply_mode целиком заменяется пустышкой» — то есть `/mode` молча не переключает
    // режим, а рапортует «Режим изменён».

    #[test]
    fn mode_command_switches_every_mode_and_persists_it() {
        let (mut app, dir) = app_for_commands();

        for (arg, expected) in [
            ("codex-only", Mode::CodexOnly),
            ("claude-only", Mode::ClaudeOnly),
            ("claude-codex", Mode::ClaudeCodex),
            ("codex-claude", Mode::CodexClaude),
        ] {
            let out = joined(&mut app, &format!("/mode {arg}"));
            assert_eq!(
                app.mode, expected,
                "«/mode {arg}» обязан ПЕРЕКЛЮЧИТЬ режим, а не только отрапортовать: {out}"
            );
            assert_eq!(app.status, format!("mode:{}", expected.as_str()));
            assert!(out.contains("Режим изменён"), "нет подтверждения: {out}");
        }

        // Режим уехал на диск: следующий запуск обязан его помнить.
        let saved = load_config(&app.config_path);
        assert_eq!(
            saved.mode,
            Mode::CodexClaude,
            "последний выбранный режим обязан сохраниться в конфиг"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn roles_command_sets_the_mode_from_the_pair() {
        let (mut app, dir) = app_for_commands();
        app.mode = Mode::CodexOnly;

        let out = joined(&mut app, "/roles claude codex");
        assert_eq!(
            app.mode,
            Mode::from_roles(Provider::Claude, Provider::Codex),
            "пара «архитектор ревьюер» обязана превратиться в режим: {out}"
        );
        assert!(
            out.contains("Роли планирования"),
            "нет подтверждения: {out}"
        );

        // Обратный порядок — ДРУГОЙ режим. Иначе роли можно было бы перепутать местами
        // и не заметить.
        let out = joined(&mut app, "/roles codex claude");
        assert_eq!(
            app.mode,
            Mode::from_roles(Provider::Codex, Provider::Claude)
        );
        assert_ne!(
            Mode::from_roles(Provider::Claude, Provider::Codex),
            Mode::from_roles(Provider::Codex, Provider::Claude),
            "порядок ролей обязан различаться — иначе тест выше ничего не проверяет"
        );
        assert!(
            out.contains("Роли планирования"),
            "нет подтверждения: {out}"
        );

        // Мусор вместо провайдера — режим не трогаем.
        let before = app.mode;
        let out = joined(&mut app, "/roles codex сосед");
        assert_eq!(
            app.mode, before,
            "неизвестный провайдер не смеет менять режим"
        );
        assert!(
            out.contains("Использование: /roles"),
            "нет подсказки: {out}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // ГРАНИЦА, ОБЪЯВЛЕННАЯ ЧЕСТНО. Мутант `|| → &&` в `App::suggestions` (фильтр по `usage` и
    // `insert`) убить НЕЛЬЗЯ, и это не лень: `normalized_command_query` возвращает None, если
    // после команды есть пробел, — значит, needle пробела не содержит НИКОГДА. А у всех команд
    // в таблице `usage` и `insert` начинаются с одного токена, и для любого needle без пробела
    // оба `starts_with` дают ОДНО И ТО ЖЕ. Проверено перебором всей таблицы: ни одной команды,
    // где они расходятся, нет. Мутант эквивалентный; `||` тут — задел на алиас, которого пока
    // не существует.
    //
    // Ещё три мутанта оставлены сознательно: `/logout`, `/auth` и `/setup` идут в
    // `open_auth_screen` → `Onboarding::new`, а тот поднимает ЖИВЫЕ auth-пробы `claude` и
    // `codex`. Тест с настоящим CLI — это флейк в наборе и платный вызов в CI. Закрывать их
    // надо иначе, отдельно.
}
