/// Лента тандема, передаётся целиком в каждый промпт (P6: усечение при росте).
pub(crate) struct TandemTranscript {
    entries: Vec<String>,
}

impl TandemTranscript {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, who: &str, phase: &str, text: &str) {
        self.entries
            .push(format!("[{who} · {phase}]\n{}", text.trim()));
    }

    pub(crate) fn render(&self) -> String {
        let full = self.entries.join("\n\n");
        if full.len() <= 12_000 || self.entries.len() <= 4 {
            return full;
        }
        // P6: оставляем первую запись + хвост (последние 3)
        let head = &self.entries[0];
        let tail = &self.entries[self.entries.len() - 3..];
        format!(
            "{head}\n\n…[ранние раунды усечены]…\n\n{}",
            tail.join("\n\n")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tandem_transcript_renders_and_truncates() {
        let mut t = TandemTranscript::new();
        t.push("Executor", "proposal 1", "short");
        assert!(t.render().contains("short"));
        for i in 0..60 {
            t.push("Critic", "round", &format!("entry {i} {}", "y".repeat(400)));
        }
        assert!(t.render().contains("усечены"));
    }

    #[test]
    fn tandem_transcript_keeps_everything_while_short() {
        // Много КОРОТКИХ записей — усечения нет (лимит по объёму, а не по числу).
        let mut t = TandemTranscript::new();
        for i in 1..=6 {
            t.push("Критик", "раунд", &format!("запись {i}"));
        }
        let render = t.render();
        assert!(!render.contains("усечены"));
        assert!(render.contains("запись 2"));

        // Четыре ДЛИННЫЕ записи — тоже целиком: обрезать нечего, это первый круг.
        let mut big = TandemTranscript::new();
        for i in 1..=4 {
            big.push(
                "Исполнитель",
                "раунд",
                &format!("запись {i} {}", "x".repeat(4000)),
            );
        }
        assert!(!big.render().contains("усечены"));
    }

    #[test]
    fn tandem_transcript_truncation_keeps_head_and_last_three() {
        let mut t = TandemTranscript::new();
        for i in 1..=5 {
            t.push(
                "Исполнитель",
                "раунд",
                &format!("запись {i} {}", "x".repeat(4000)),
            );
        }
        let render = t.render();
        assert!(render.contains("усечены"));
        assert!(
            render.contains("запись 1"),
            "первая запись — задача — остаётся"
        );
        assert!(!render.contains("запись 2"), "ранние раунды выброшены");
        for i in 3..=5 {
            assert!(
                render.contains(&format!("запись {i}")),
                "хвост — последние три"
            );
        }
    }

    // --- решение о повторе в чате ---
}
