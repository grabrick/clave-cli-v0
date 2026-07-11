use crate::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Theme {
    Purple,
    Cyan,
    Rose,
    Amber,
    Mono,
}

impl Theme {
    /// Все темы по порядку — единый источник для перебора (shifted и пр.), чтобы список
    /// не дублировался внутри функций и не рассинхронился с вариантами enum.
    pub(crate) const ALL: &'static [Theme] = &[
        Theme::Purple,
        Theme::Cyan,
        Theme::Rose,
        Theme::Amber,
        Theme::Mono,
    ];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Theme::Purple => "purple",
            Theme::Cyan => "cyan",
            Theme::Rose => "rose",
            Theme::Amber => "amber",
            Theme::Mono => "mono",
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Theme::Purple => "Purple",
            Theme::Cyan => "Cyan",
            Theme::Rose => "Rose",
            Theme::Amber => "Amber",
            Theme::Mono => "Mono",
        }
    }

    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value {
            "purple" | "violet" => Some(Theme::Purple),
            "cyan" | "blue" => Some(Theme::Cyan),
            "rose" | "pink" => Some(Theme::Rose),
            "amber" | "yellow" => Some(Theme::Amber),
            "mono" | "gray" | "grey" => Some(Theme::Mono),
            _ => None,
        }
    }

    pub(crate) fn shifted(self, direction: isize) -> Self {
        let current = Self::ALL
            .iter()
            .position(|theme| *theme == self)
            .unwrap_or_default();
        let next = if direction < 0 {
            current.saturating_sub(1)
        } else {
            (current + 1).min(Self::ALL.len() - 1)
        };
        Self::ALL[next]
    }

    pub(crate) fn accent(self) -> Color {
        match self {
            Theme::Purple => Color::Indexed(141),
            Theme::Cyan => Color::Indexed(80),
            Theme::Rose => Color::Indexed(205),
            Theme::Amber => Color::Indexed(220),
            Theme::Mono => Color::Indexed(250),
        }
    }

    pub(crate) fn accent_soft(self) -> Color {
        match self {
            Theme::Purple => Color::Indexed(183),
            Theme::Cyan => Color::Indexed(159),
            Theme::Rose => Color::Indexed(218),
            Theme::Amber => Color::Indexed(229),
            Theme::Mono => Color::Indexed(246),
        }
    }

    pub(crate) fn accent_dim(self) -> Color {
        match self {
            Theme::Purple => Color::Indexed(97),
            Theme::Cyan => Color::Indexed(30),
            Theme::Rose => Color::Indexed(132),
            Theme::Amber => Color::Indexed(136),
            Theme::Mono => Color::Indexed(240),
        }
    }

    pub(crate) fn accent_bg(self) -> Color {
        match self {
            Theme::Purple => Color::Indexed(60),
            Theme::Cyan => Color::Indexed(24),
            Theme::Rose => Color::Indexed(53),
            Theme::Amber => Color::Indexed(94),
            Theme::Mono => Color::Indexed(238),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_themes_round_trip_and_stay_in_sync() {
        // Каждая тема из ALL кодируется/декодируется обратно в себя — ловит опечатку в
        // as_str/from_str и подтверждает, что ALL перечисляет реальные варианты.
        for &theme in Theme::ALL {
            assert_eq!(Theme::from_str(theme.as_str()), Some(theme));
        }
        // Напоминание держать ALL в синхроне с вариантами enum при добавлении темы.
        assert_eq!(Theme::ALL.len(), 5);
    }
}
