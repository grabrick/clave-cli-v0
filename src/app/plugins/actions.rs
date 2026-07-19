use super::*;

/// Отложенное действие над плагином, ждущее подтверждения (установка/удаление меняют окружение).
pub(crate) struct PendingPluginAction {
    pub(crate) action: PluginAction,
    pub(crate) entry: PluginEntry,
}

impl App {
    /// Приём догруженного codex-списка: добавляем к уже показанному claude, снимаем флаг. Если
    /// действие было над codex-плагином — теперь он в списке, возвращаем на него курсор (финально).
    pub(crate) fn plugins_loaded(&mut self, mut codex: Vec<PluginEntry>) {
        self.plugins.append(&mut codex);
        self.plugins_loading = false;
        if self.plugins_reselect.is_some() {
            self.restore_plugins_selection();
            self.plugins_reselect = None;
        }
    }

    /// Выбранный в панели плагин (по курсору в отфильтрованном списке).
    fn selected_plugin(&self) -> Option<PluginEntry> {
        self.filtered_plugins()
            .get(self.plugins_index)
            .map(|p| (*p).clone())
    }

    /// Клавиша `Enter`: доступный плагин → установить, установленный → удалить (через
    /// подтверждение — оба необратимо-затратны).
    pub(crate) fn plugin_enter(&mut self) {
        let Some(entry) = self.selected_plugin() else {
            return;
        };
        let action = if entry.installed {
            PluginAction::Uninstall
        } else {
            PluginAction::Install
        };
        // Обе ветки необратимы → всегда через подтверждение.
        self.plugins_confirm = Some(PendingPluginAction { action, entry });
    }

    /// Вкл/выкл выбранного (обратимо, без подтверждения). На доступном (не установлен) — no-op.
    pub(crate) fn plugin_toggle(&mut self) {
        let Some(entry) = self.selected_plugin() else {
            return;
        };
        if !entry.installed {
            return;
        }
        let action = if entry.enabled {
            PluginAction::Disable
        } else {
            PluginAction::Enable
        };
        self.run_plugin_action(action, &entry);
    }

    /// Обновить выбранный установленный плагин (без подтверждения).
    pub(crate) fn plugin_update(&mut self) {
        if let Some(entry) = self.selected_plugin() {
            if entry.installed {
                self.run_plugin_action(PluginAction::Update, &entry);
            }
        }
    }

    /// Подтвердить отложенное действие (Enter в строке подтверждения).
    pub(crate) fn confirm_plugin_action(&mut self) {
        if let Some(pending) = self.plugins_confirm.take() {
            self.run_plugin_action(pending.action, &pending.entry);
        }
    }

    pub(crate) fn cancel_plugin_action(&mut self) {
        self.plugins_confirm = None;
    }

    /// Спавнит команду действия провайдера; по завершении шлёт `PluginActionDone` → refresh.
    fn run_plugin_action(&mut self, action: PluginAction, entry: &PluginEntry) {
        // Запоминаем плагин, чтобы после refresh вернуть курсор на него (вкл/выкл его не двигают).
        self.plugins_reselect = Some(entry.qualified_name());
        let mut cmd = match entry.provider {
            Provider::Claude => claude_action_cmd(action, entry),
            Provider::Codex => codex_action_cmd(action, entry),
        };
        self.status = self
            .lang
            .choose("плагин: выполняю…", "plugin: working…")
            .to_string();
        let tx = self.tx.clone();
        (self.run_hooks.spawn)(
            self.tx.clone(),
            Box::new(move || {
                let _ = cmd.output();
                let _ = tx.send(WorkerEvent::PluginActionDone);
            }),
        );
    }

    /// Действие завершено — перезагружаем список (статусы install/enabled могли измениться) и
    /// возвращаем курсор на тот же плагин (claude уже перезагружен; codex догрузится событием).
    pub(crate) fn plugin_action_done(&mut self) {
        self.plugins = load_claude_plugins(&self.claude_home);
        self.plugin_details = load_claude_plugin_details(&self.claude_home);
        self.plugin_updates = load_claude_plugin_updates(&self.claude_home);
        self.plugins_loading = true;
        self.restore_plugins_selection();
        self.spawn_codex_plugins_load();
    }

    /// Возвращает курсор на плагин из `plugins_reselect`, если он ещё в текущем (таб+провайдер)
    /// списке; иначе прижимает индекс к длине (плагин удалён или сменил таб). Для codex цель
    /// найдётся только после `plugins_loaded` — до тех пор `plugins_reselect` сохраняется.
    fn restore_plugins_selection(&mut self) {
        if let Some(name) = self.plugins_reselect.clone() {
            if let Some(pos) = self
                .filtered_plugins()
                .iter()
                .position(|p| p.qualified_name() == name)
            {
                self.plugins_index = pos;
                self.plugins_reselect = None;
                return;
            }
        }
        let last = self.filtered_plugins().len().saturating_sub(1);
        self.plugins_index = self.plugins_index.min(last);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::plugins::testkit::*;

    #[test]
    fn plugins_loaded_appends_codex_and_clears_loading() {
        let home = env::temp_dir().join(format!("clave-loaded-home-{}", std::process::id()));
        let _ = fs::create_dir_all(&home);
        seed_claude_home(&home);
        let (mut app, dir) = plugins_app(&home);
        app.open_plugins_panel();

        app.plugins_loaded(vec![PluginEntry {
            provider: Provider::Codex,
            name: "documents".to_string(),
            marketplace: "openai".to_string(),
            installed: true,
            enabled: true,
            version: None,
        }]);

        assert!(!app.plugins_loading, "флаг снят");
        assert_eq!(count_of(&app, Provider::Codex), 1);
        assert_eq!(count_of(&app, Provider::Claude), 1, "claude на месте");

        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn enter_on_installed_asks_to_uninstall_then_cancel_clears_it() {
        let home = env::temp_dir().join(format!("clave-act-home-{}", std::process::id()));
        let _ = fs::create_dir_all(&home);
        seed_claude_home(&home);
        let (mut app, dir) = plugins_app(&home);
        app.open_plugins_panel(); // context7 установлен
        app.plugins_tab = PluginsTab::Installed; // таб установленных — там он и виден

        app.plugins_index = 0;
        app.plugin_enter();
        let pending = app
            .plugins_confirm
            .as_ref()
            .expect("действие ждёт подтверждения");
        assert_eq!(
            pending.action,
            PluginAction::Uninstall,
            "установленный плагин → предложить удаление"
        );

        app.cancel_plugin_action();
        assert!(app.plugins_confirm.is_none(), "отмена сняла подтверждение");

        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn toggle_installed_runs_action_without_confirm() {
        let home = env::temp_dir().join(format!("clave-tog-home-{}", std::process::id()));
        let _ = fs::create_dir_all(&home);
        seed_claude_home(&home);
        let (mut app, dir) = plugins_app(&home);
        app.open_plugins_panel();
        app.plugins_tab = PluginsTab::Installed;
        app.plugins_index = 0;

        app.plugin_toggle(); // context7 включён → выключить, без подтверждения (обратимо)
        assert!(
            app.plugins_confirm.is_none(),
            "вкл/выкл не требует подтверждения"
        );
        assert!(
            app.status.contains("выполня") || app.status.contains("working"),
            "статус действия: {}",
            app.status
        );

        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn action_returns_cursor_to_the_same_plugin_not_the_top() {
        let (mut app, dir) = plugins_app(&env::temp_dir());
        app.plugins_tab = PluginsTab::Installed;
        app.plugins = vec![
            plugin_entry(Provider::Claude, "aaa", true),
            plugin_entry(Provider::Claude, "bbb", true),
            plugin_entry(Provider::Claude, "ccc", true),
        ];
        // Действие над средним плагином запомнило его; refresh сбросил бы курсор на 0.
        app.plugins_reselect = Some("bbb@m".to_string());
        app.plugins_index = 0;
        app.restore_plugins_selection();
        assert_eq!(
            app.plugins_index, 1,
            "курсор вернулся на bbb, а не на первый"
        );
        assert!(app.plugins_reselect.is_none(), "цель снята после возврата");

        // Плагин ушёл из списка (удалён) → индекс прижат к длине, без паники.
        app.plugins = vec![plugin_entry(Provider::Claude, "aaa", true)];
        app.plugins_index = 5;
        app.plugins_reselect = Some("gone@m".to_string());
        app.restore_plugins_selection();
        assert_eq!(app.plugins_index, 0, "прижат к длине списка");

        let _ = fs::remove_dir_all(&dir);
    }
}
