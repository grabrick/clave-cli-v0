use super::*;

mod chat;
mod task;

impl App {
    pub(crate) fn resolved_work_dir(&self) -> PathBuf {
        resolve_work_dir(&self.work_dir, &launch_work_dir())
    }
}

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;

    pub(crate) fn noop_spawn(_tx: Sender<WorkerEvent>, _body: Box<dyn FnOnce() + Send + 'static>) {}

    /// App с ФЕЙКОВЫМИ хуками: спавн — no-op (тело дропается, реального воркера нет),
    /// логин — «залогинен». Позволяет наблюдать диспетч start_chat и guard-ы без
    /// подпроцессов и живых auth-проб.
    pub(crate) fn runs_app() -> App {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "clave-runs-{}-{}",
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
        app.run_hooks = RunHooks {
            spawn: noop_spawn,
            authenticated: |_| true,
        };
        app
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    // --- Мутант mod.rs:178 (`real_spawn → ()`): дефолтный хук обязан выполнить тело ---
    // Берём фн-указатель через RunHooks::real(), спавним тело, шлющее сигнал, и ждём с таймаутом.

    #[test]
    fn real_spawn_hook_actually_runs_the_body() {
        let hooks = RunHooks::real();
        let (tx, _rx) = mpsc::channel::<WorkerEvent>();
        let (done_tx, done_rx) = mpsc::channel();
        (hooks.spawn)(
            tx,
            Box::new(move || {
                let _ = done_tx.send(());
            }),
        );
        done_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("real_spawn обязан выполнить тело в потоке");
    }
}
