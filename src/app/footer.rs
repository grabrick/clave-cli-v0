use super::*;

/// Как часто перепроверяем git-ref: ветка меняется прямо во время сессии (в том числе
/// самим `/dev`), но спавнить `git` 10 раз в секунду вслед за тиком цикла незачем.
const GIT_REF_TTL: Duration = Duration::from_secs(2);

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
    /// Обновляет индикатор git-ref (с TTL). Сбрось `git_ref_checked_at` в `None`, чтобы
    /// перечитать немедленно — например, после смены рабочей директории.
    pub(crate) fn refresh_git_ref(&mut self) {
        let fresh = self
            .git_ref_checked_at
            .map(|checked_at| checked_at.elapsed() < GIT_REF_TTL)
            .unwrap_or(false);
        if fresh {
            return;
        }

        self.git_ref_checked_at = Some(Instant::now());
        self.git_ref = detect_git_ref(&self.resolved_work_dir());
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

    #[test]
    fn plain_directory_has_no_git_ref() {
        let dir = temp_repo("plain");
        let got = detect_git_ref(&dir);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(got, None);
    }
}
