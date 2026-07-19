use super::*;

mod provider;
mod width;
mod wrap;
pub(crate) use provider::*;
pub(crate) use width::*;
pub(crate) use wrap::*;

pub(crate) fn composer_height(app: &App, width: u16) -> u16 {
    let lines = input_lines_wrapped(&app.input, width).len() as u16;
    // +2 служебные строки: верхняя полоска (со встроенной плашкой) и нижняя полоска.
    (lines + 2).clamp(3, 10)
}

pub(crate) fn initial_transcript(_lang: Language) -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn composer_app(input: &str) -> App {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);

        let dir = std::env::temp_dir().join(format!(
            "clave-helpers-{}-{}",
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
        app.input = input.to_string();
        app
    }

    /// Высота = строки ввода + 2 служебные, зажатые в [3, 10].
    #[test]
    fn composer_height_is_input_lines_plus_two_clamped_to_three_and_ten() {
        // Пустой ввод: 1 + 2 = 3, нижний кламп. Валит `->0` и `->1` (43:5).
        assert_eq!(composer_height(&composer_app(""), 20), 3);
        // Три строки: 3 + 2 = 5. Здесь `+` и `*` (45:12) расходятся: 3*2=6 ≠ 5.
        assert_eq!(composer_height(&composer_app("a\nb\nc"), 20), 5);
        // Пятнадцать строк: 15 + 2 = 17, верхний кламп режет до 10.
        let tall = "x\n".repeat(15);
        assert_eq!(composer_height(&composer_app(&tall), 20), 10);
    }
}
