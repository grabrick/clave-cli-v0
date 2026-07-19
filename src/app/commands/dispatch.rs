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
            "/plugins" => self.open_plugins_panel(),
            "/clear" => {
                let arg = rest.trim();
                if arg.is_empty() {
                    // Как /clear в Claude: контекст И текущий именованный чат уходят.
                    self.clear_current_chat();
                } else if arg == "history" {
                    self.clear_all_chats();
                } else {
                    // Неизвестный аргумент НЕ трактуем как «удалить»: показываем подсказку.
                    // Иначе опечатка (/clear all, /clear histor) молча стёрла бы текущий чат.
                    self.push_system(self.lang.choose(
                        "Использование: /clear (текущий чат) · /clear history (все чаты)",
                        "Usage: /clear (current chat) · /clear history (all chats)",
                    ));
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
                | "/plugins"
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::testkit::*;

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

    #[test]
    fn clear_with_an_unknown_argument_keeps_the_current_chat() {
        let (mut app, _dir) = app_for_commands();
        app.push_system("важное сообщение"); // создаёт файл текущего чата
        assert!(app.chat_path.exists(), "файл текущего чата создан");

        let echoed = run(&mut app, "/clear all"); // опечатка / неизвестный аргумент

        assert!(
            app.chat_path.exists(),
            "неизвестный аргумент /clear НЕ должен молча удалять текущий чат"
        );
        assert!(
            echoed.iter().any(|l| l.contains("Использование")),
            "вместо удаления показана подсказка: {echoed:?}"
        );
    }

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

    // ─────────────────────────── /mode и /roles ───────────────────────────
    //
    // `/mode` покрыт — но ПОСЛЕ шва раннера, и историю стоит помнить. Раньше `/mode` был НЕ
    // покрыт, и это стоило CI: здесь стоял мой тест, и он падал на машине без провайдеров.
    // Причина — последним вызовом `apply_mode` идёт `ensure_auth_ready_for_current_mode()`, а тот
    // делал `Onboarding::new(mode)`, то есть поднимал НАСТОЯЩИЕ `claude` и `codex`. Я проверил
    // цепочку `apply_mode` → `set_mode` → `save_current_config`, объявил путь чистым и ОСТАНОВИЛСЯ
    // ЗА СТРОКУ ДО КОНЦА. Урок остаётся: проверять цепочку надо ДО КОНЦА, а не до места, где она
    // подтверждает удобное мнение.
    //
    // Что изменилось: шов раннера провёл готовность логина через `run_hooks.authenticated`. Теперь
    // `ensure_auth_ready_for_current_mode` на пути «залогинен» возвращается ДО `Onboarding::new`
    // (см. app/onboarding.rs). Значит с фейком `authenticated = |_| true` вся цепочка `/mode` идёт
    // без единого спавна процесса — путь чист в ЛЮБОМ окружении, включая CI без провайдеров.
    // Каждый тест ниже стартует с ДРУГОГО режима и убивает delete-arm своего плеча диспетча:
    // при удалении плеча команда ушла бы в catch-all и режим остался бы прежним.

    #[test]
    fn mode_codex_only_switches_the_running_mode() {
        let (mut app, dir) = app_for_commands();
        app.run_hooks.authenticated = |_| true; // логин готов → ensure_auth без Onboarding::new
        app.mode = Mode::ClaudeOnly; // старт ОТЛИЧАЕТСЯ от цели

        let out = joined(&mut app, "/mode codex-only");

        assert_eq!(
            app.mode,
            Mode::CodexOnly,
            "плечо codex-only обязано сменить режим; delete-arm увёл бы в catch-all: {out}"
        );
        assert!(
            out.contains("Режим изменён"),
            "нет подтверждения смены: {out}"
        );
        assert!(
            app.onboarding.is_none(),
            "готовый логин не должен открывать экран авторизации"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mode_claude_only_switches_the_running_mode() {
        let (mut app, dir) = app_for_commands();
        app.run_hooks.authenticated = |_| true;
        app.mode = Mode::CodexOnly;

        let out = joined(&mut app, "/mode claude-only");

        assert_eq!(
            app.mode,
            Mode::ClaudeOnly,
            "плечо claude-only обязано сменить режим; delete-arm увёл бы в catch-all: {out}"
        );
        assert!(
            out.contains("Режим изменён"),
            "нет подтверждения смены: {out}"
        );
        assert!(
            app.onboarding.is_none(),
            "готовый логин не должен открывать экран авторизации"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mode_claude_codex_switches_the_running_mode() {
        let (mut app, dir) = app_for_commands();
        app.run_hooks.authenticated = |_| true;
        app.mode = Mode::CodexOnly;

        let out = joined(&mut app, "/mode claude-codex");

        assert_eq!(
            app.mode,
            Mode::ClaudeCodex,
            "плечо claude-codex обязано сменить режим; delete-arm увёл бы в catch-all: {out}"
        );
        assert!(
            out.contains("Режим изменён"),
            "нет подтверждения смены: {out}"
        );
        assert!(
            app.onboarding.is_none(),
            "готовый логин не должен открывать экран авторизации"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mode_codex_claude_switches_the_running_mode() {
        let (mut app, dir) = app_for_commands();
        app.run_hooks.authenticated = |_| true;
        app.mode = Mode::ClaudeOnly;

        let out = joined(&mut app, "/mode codex-claude");

        assert_eq!(
            app.mode,
            Mode::CodexClaude,
            "плечо codex-claude обязано сменить режим; delete-arm увёл бы в catch-all: {out}"
        );
        assert!(
            out.contains("Режим изменён"),
            "нет подтверждения смены: {out}"
        );
        assert!(
            app.onboarding.is_none(),
            "готовый логин не должен открывать экран авторизации"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // Мусорный аргумент /mode не трогает режим и подсказывает синтаксис (catch-all плечо).
    #[test]
    fn mode_with_an_unknown_argument_keeps_the_mode() {
        let (mut app, dir) = app_for_commands();
        app.run_hooks.authenticated = |_| true;
        app.mode = Mode::ClaudeCodex;

        let out = joined(&mut app, "/mode сосед");

        assert_eq!(
            app.mode,
            Mode::ClaudeCodex,
            "неизвестный режим не смеет менять mode: {out}"
        );
        assert!(
            out.contains("Использование"),
            "нет подсказки по синтаксису: {out}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // `/roles` покрыт: `set_roles` заканчивается на `push_command_result` и проверки авторизации
    // не зовёт — проверено прогоном с `CLAVE_CLAUDE=/nonexistent`.

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
