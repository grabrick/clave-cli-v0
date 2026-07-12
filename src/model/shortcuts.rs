use super::Language;

/// Хоткей: клавиши + локализованное описание.
pub(crate) struct ShortcutSpec {
    pub(crate) keys: &'static str,
    pub(crate) ru: &'static str,
    pub(crate) en: &'static str,
}

impl ShortcutSpec {
    pub(crate) fn describe(&self, lang: Language) -> &'static str {
        lang.choose(self.ru, self.en)
    }
}

/// Группа хоткеев с локализованным заголовком — панель `?` рисует их секциями.
pub(crate) struct ShortcutGroup {
    pub(crate) ru: &'static str,
    pub(crate) en: &'static str,
    pub(crate) items: &'static [ShortcutSpec],
}

impl ShortcutGroup {
    pub(crate) fn title(&self, lang: Language) -> &'static str {
        lang.choose(self.ru, self.en)
    }
}

/// Клавиши переключения режима чата (показываются и в футере, и в подсказках).
pub(crate) const MODE_SWITCH_KEYS: &str = "Shift+Tab";

/// Сгруппированный каталог хоткеев для панели `?`. Порядок = порядок вывода: первая
/// половина групп уходит в левую колонку, вторая — в правую (см. `ui/shortcuts.rs`).
pub(crate) const SHORTCUT_GROUPS: &[ShortcutGroup] = &[
    ShortcutGroup {
        ru: "Отправка",
        en: "Compose",
        items: &[
            ShortcutSpec {
                keys: "Enter",
                ru: "отправить",
                en: "send",
            },
            ShortcutSpec {
                keys: "Shift/Alt+Enter",
                ru: "новая строка (Ctrl+J)",
                en: "newline (Ctrl+J)",
            },
            ShortcutSpec {
                keys: "Tab",
                ru: "автодополнение",
                en: "complete",
            },
        ],
    },
    ShortcutGroup {
        ru: "Правка",
        en: "Edit",
        items: &[
            ShortcutSpec {
                keys: "Ctrl+W/U/K",
                ru: "удалить",
                en: "delete",
            },
            ShortcutSpec {
                keys: "Esc",
                ru: "сброс",
                en: "clear",
            },
        ],
    },
    ShortcutGroup {
        ru: "Навигация",
        en: "Navigate",
        items: &[
            ShortcutSpec {
                keys: "Ctrl+A/E",
                ru: "начало / конец",
                en: "start / end",
            },
            ShortcutSpec {
                keys: "Alt+←→",
                ru: "по словам",
                en: "by word",
            },
            ShortcutSpec {
                keys: "↑↓",
                ru: "история",
                en: "history",
            },
            ShortcutSpec {
                keys: "Ctrl+R",
                ru: "поиск",
                en: "search",
            },
        ],
    },
    ShortcutGroup {
        ru: "Сессия",
        en: "Session",
        items: &[
            ShortcutSpec {
                keys: MODE_SWITCH_KEYS,
                ru: "режим",
                en: "mode",
            },
            ShortcutSpec {
                keys: "Ctrl+C ×2",
                ru: "выход",
                en: "exit",
            },
            ShortcutSpec {
                keys: "?",
                ru: "скрыть",
                en: "hide",
            },
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_groups_filled_and_include_mode_switch() {
        assert!(!SHORTCUT_GROUPS.is_empty());
        for group in SHORTCUT_GROUPS {
            assert!(!group.ru.is_empty() && !group.en.is_empty());
            assert!(!group.items.is_empty());
            for spec in group.items {
                assert!(!spec.keys.is_empty() && !spec.ru.is_empty() && !spec.en.is_empty());
            }
        }
        assert!(SHORTCUT_GROUPS
            .iter()
            .flat_map(|g| g.items)
            .any(|spec| spec.keys == MODE_SWITCH_KEYS));
    }
}
