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
                | "/dev"
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
            "/dev" => {
                if rest.trim().is_empty() {
                    self.push_system(
                        self.lang
                            .choose("Использование: /dev <задача>", "Usage: /dev <task>"),
                    );
                } else {
                    self.begin_dev(rest.trim().to_string());
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
                | "/dev"
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
        // Индикатор футера — про НОВЫЙ каталог, и уже в этом кадре, а не через TTL.
        self.git_ref_checked_at = None;
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
}
