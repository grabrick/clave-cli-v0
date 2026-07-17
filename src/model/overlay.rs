#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Overlay {
    #[default]
    None,
    Effort,
    Settings,
    Chats,
    Plugins,
    Shortcuts,
    Search,
}

impl Overlay {
    pub(crate) fn is_open(self) -> bool {
        self != Overlay::None
    }

    /// Полноэкранные модалки — рисуются во временном alt-screen (инвариант 4).
    /// Палитра/?/search/gate — НЕ модалки, они inline в живом viewport.
    pub(crate) fn is_modal(self) -> bool {
        matches!(
            self,
            Overlay::Effort | Overlay::Settings | Overlay::Chats | Overlay::Plugins
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_is_not_open_others_are() {
        assert!(!Overlay::None.is_open());
        assert!(Overlay::Effort.is_open());
        assert!(Overlay::Settings.is_open());
        assert_eq!(Overlay::default(), Overlay::None);
    }
}
