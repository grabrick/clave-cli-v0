use super::Language;

#[derive(Clone, Copy)]
pub(crate) struct CommandSpec {
    pub(crate) usage: &'static str,
    pub(crate) insert: &'static str,
    pub(crate) description_en: &'static str,
    pub(crate) description_ru: &'static str,
}

pub(crate) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        usage: "/brainstorm",
        insert: "/brainstorm ",
        description_en: "Explore options before implementation",
        description_ru: "Исследовать варианты перед реализацией",
    },
    CommandSpec {
        usage: "/blueprint",
        insert: "/blueprint ",
        description_en: "Turn the context into a step-by-step plan",
        description_ru: "Превратить контекст в пошаговый план",
    },
    CommandSpec {
        usage: "/finish-branch",
        insert: "/finish-branch ",
        description_en: "Decide how to complete and polish the branch",
        description_ru: "Довести ветку до завершения (тесты, доки, пуш)",
    },
    CommandSpec {
        usage: "/split-work",
        insert: "/split-work ",
        description_en: "Split a task into parallel workstreams",
        description_ru: "Разбить задачу на параллельные потоки",
    },
    CommandSpec {
        usage: "/worktrees",
        insert: "/worktrees ",
        description_en: "Plan parallel work via git worktrees",
        description_ru: "Спланировать параллельную работу через git worktrees",
    },
    CommandSpec {
        usage: "/add-dir",
        insert: "/add-dir ",
        description_en: "Add a new working directory",
        description_ru: "Добавить рабочую директорию",
    },
    CommandSpec {
        usage: "/advisor",
        insert: "/advisor ",
        description_en: "Consult a stronger model for guidance",
        description_ru: "Попросить сильную модель о совете",
    },
    CommandSpec {
        usage: "/agents",
        insert: "/agents",
        description_en: "Manage agent configurations",
        description_ru: "Управлять конфигурациями агентов",
    },
    CommandSpec {
        usage: "/autofix-pr",
        insert: "/autofix-pr",
        description_en: "Monitor and autofix issues with the current PR",
        description_ru: "Отслеживать и исправлять проблемы в текущем PR",
    },
    CommandSpec {
        usage: "/background",
        insert: "/background",
        description_en: "Send this session to the background",
        description_ru: "Отправить сессию в фон",
    },
    CommandSpec {
        usage: "/branch",
        insert: "/branch",
        description_en: "Create a branch of the current conversation",
        description_ru: "Создать ветку текущего разговора",
    },
    CommandSpec {
        usage: "/btw",
        insert: "/btw ",
        description_en: "Ask a quick side question",
        description_ru: "Задать быстрый побочный вопрос",
    },
    CommandSpec {
        usage: "/plan <task>",
        insert: "/plan ",
        description_en: "Run the multi-agent Clave planning loop",
        description_ru: "Запустить multi-agent планирование Clave",
    },
    CommandSpec {
        usage: "/dev <task>",
        insert: "/dev ",
        description_en: "Self-dev: run external clave-dev on the current repo",
        description_ru: "Самопиление: внешний clave-dev на текущем репозитории",
    },
    CommandSpec {
        usage: "/clear",
        insert: "/clear",
        description_en: "Start fresh with empty context",
        description_ru: "Очистить контекст",
    },
    CommandSpec {
        usage: "/clear history",
        insert: "/clear history",
        description_en: "Delete all saved chats except current",
        description_ru: "Удалить все чаты, кроме текущего",
    },
    CommandSpec {
        usage: "/new",
        insert: "/new",
        description_en: "Start a new saved chat",
        description_ru: "Начать новый сохранённый чат",
    },
    CommandSpec {
        usage: "/name <title>",
        insert: "/name ",
        description_en: "Name the current chat",
        description_ru: "Задать имя текущему чату",
    },
    CommandSpec {
        usage: "/rename <title>",
        insert: "/rename ",
        description_en: "Rename the current chat",
        description_ru: "Переименовать текущий чат",
    },
    CommandSpec {
        usage: "/chats",
        insert: "/chats",
        description_en: "Show saved chats",
        description_ru: "Показать сохранённые чаты",
    },
    CommandSpec {
        usage: "/chats clear",
        insert: "/chats clear",
        description_en: "Delete tiny saved chats",
        description_ru: "Удалить мелкие сохранённые чаты",
    },
    CommandSpec {
        usage: "/plugins",
        insert: "/plugins",
        description_en: "Manage Claude/Codex plugins",
        description_ru: "Плагины Claude и Codex",
    },
    CommandSpec {
        usage: "/resume <id>",
        insert: "/resume ",
        description_en: "Resume a saved chat",
        description_ru: "Открыть сохранённый чат",
    },
    CommandSpec {
        usage: "/retry",
        insert: "/retry",
        description_en: "Repeat the last chat request",
        description_ru: "Повторить последний запрос",
    },
    CommandSpec {
        usage: "/export",
        insert: "/export",
        description_en: "Export chat to a markdown file",
        description_ru: "Экспортировать чат в markdown",
    },
    CommandSpec {
        usage: "/search",
        insert: "/search",
        description_en: "Search the transcript (Ctrl+R)",
        description_ru: "Поиск по ленте (Ctrl+R)",
    },
    CommandSpec {
        usage: "/color",
        insert: "/color ",
        description_en: "Set the prompt bar color",
        description_ru: "Изменить цвет строки ввода",
    },
    CommandSpec {
        usage: "/effort",
        insert: "/effort",
        description_en: "Adjust model effort level",
        description_ru: "Настроить уровень усилия модели",
    },
    CommandSpec {
        usage: "/settings",
        insert: "/settings",
        description_en: "Open model, theme, and role settings",
        description_ru: "Открыть настройки моделей, темы и ролей",
    },
    CommandSpec {
        usage: "/chat-model codex|claude",
        insert: "/chat-model ",
        description_en: "Choose the model for plain messages",
        description_ru: "Выбрать модель для простых сообщений",
    },
    CommandSpec {
        usage: "/theme purple|cyan|rose|amber|mono",
        insert: "/theme ",
        description_en: "Choose terminal color palette",
        description_ru: "Выбрать цветовую гамму терминала",
    },
    CommandSpec {
        usage: "/roles <executor> <reviewer>",
        insert: "/roles ",
        description_en: "Choose planning executor and reviewer",
        description_ru: "Выбрать исполнителя и ревьюера планирования",
    },
    CommandSpec {
        usage: "/logout",
        insert: "/logout",
        description_en: "Return to CLI authentication",
        description_ru: "Вернуться к авторизации CLI",
    },
    CommandSpec {
        usage: "/help",
        insert: "/help",
        description_en: "Show available commands",
        description_ru: "Показать доступные команды",
    },
    CommandSpec {
        usage: "/lang ru|en",
        insert: "/lang ",
        description_en: "Switch interface language",
        description_ru: "Переключить язык интерфейса",
    },
    CommandSpec {
        usage: "/mode codex-only",
        insert: "/mode codex-only",
        description_en: "Use Codex for both architect and reviewer",
        description_ru: "Использовать Codex как архитектора и ревьюера",
    },
    CommandSpec {
        usage: "/mode claude-only",
        insert: "/mode claude-only",
        description_en: "Use Claude for both architect and reviewer",
        description_ru: "Использовать Claude как архитектора и ревьюера",
    },
    CommandSpec {
        usage: "/mode claude-codex",
        insert: "/mode claude-codex",
        description_en: "Use Claude as architect and Codex as reviewer",
        description_ru: "Использовать Claude как архитектора и Codex как ревьюера",
    },
    CommandSpec {
        usage: "/mode codex-claude",
        insert: "/mode codex-claude",
        description_en: "Use Codex as architect and Claude as reviewer",
        description_ru: "Использовать Codex как архитектора и Claude как ревьюера",
    },
    CommandSpec {
        usage: "/rounds <n>",
        insert: "/rounds ",
        description_en: "Set review loop limit",
        description_ru: "Задать лимит раундов ревью",
    },
    CommandSpec {
        usage: "/out <dir>",
        insert: "/out ",
        description_en: "Set artifact directory",
        description_ru: "Задать папку артефактов",
    },
    CommandSpec {
        usage: "/status",
        insert: "/status",
        description_en: "Print session state",
        description_ru: "Показать состояние сессии",
    },
    CommandSpec {
        usage: "/cost",
        insert: "/cost",
        description_en: "Show model usage and cost",
        description_ru: "Показать расход моделей и стоимость",
    },
    CommandSpec {
        usage: "/version",
        insert: "/version",
        description_en: "Show version and environment info",
        description_ru: "Показать версию и информацию об инструменте",
    },
    CommandSpec {
        usage: "/uptime",
        insert: "/uptime",
        description_en: "Show how long the session has been running",
        description_ru: "Показать, сколько работает сессия",
    },
    CommandSpec {
        usage: "/setup",
        insert: "/setup",
        description_en: "Open first-run setup again",
        description_ru: "Открыть стартовую настройку заново",
    },
    CommandSpec {
        usage: "/quit",
        insert: "/quit",
        description_en: "Exit Clave",
        description_ru: "Выйти из Clave",
    },
];

impl CommandSpec {
    pub(crate) fn description(self, lang: Language) -> &'static str {
        match lang {
            Language::En => self.description_en,
            Language::Ru => self.description_ru,
        }
    }

    pub(crate) fn command_token(self) -> &'static str {
        self.usage.split_whitespace().next().unwrap_or(self.usage)
    }
}

pub(crate) fn normalized_command_query(input: &str) -> Option<String> {
    let trimmed = input.trim_start();
    if trimmed.is_empty() {
        return None;
    }

    let token_end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    if token_end < trimmed.len() {
        return None;
    }

    let normalized = normalize_ru_keyboard_layout(trimmed);
    normalized.starts_with('/').then_some(normalized)
}

pub(crate) fn normalize_command_line_for_execution(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let token_end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    let token = &trimmed[..token_end];
    let rest = trimmed[token_end..].trim_start();
    let normalized_token = normalize_ru_keyboard_layout(token);

    if is_known_command_token(&normalized_token) {
        let rest = normalize_command_rest(&normalized_token, rest);
        return Some(if rest.is_empty() {
            normalized_token
        } else {
            format!("{normalized_token} {rest}")
        });
    }

    token.starts_with('/').then(|| trimmed.to_string())
}

pub(crate) fn normalized_plain_command(input: &str) -> String {
    normalize_ru_keyboard_layout(input.trim())
}

fn is_known_command_token(token: &str) -> bool {
    COMMANDS
        .iter()
        .any(|command| command.command_token() == token)
        // Намеренные скрытые алиасы к каноническим командам (в палитру/справку не выносим):
        // /language→/lang, /clave→/plan, /auth→/logout, /exit→/quit. Их обрабатывает
        // App::handle_command; здесь распознаём как команду, а не как обычный ввод.
        || matches!(token, "/language" | "/clave" | "/auth" | "/exit")
}

fn normalize_command_rest(command: &str, rest: &str) -> String {
    match command {
        "/lang" | "/language" => {
            let normalized = normalize_ru_keyboard_layout(rest);
            match normalized.as_str() {
                "ru" | "en" | "eng" | "english" | "russian" => normalized,
                _ => rest.to_string(),
            }
        }
        "/mode" | "/chat-model" | "/theme" | "/color" | "/roles" => {
            normalize_ru_keyboard_layout(rest)
        }
        _ => rest.to_string(),
    }
}

fn normalize_ru_keyboard_layout(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for (index, ch) in input.chars().enumerate() {
        if index == 0 && ch == '.' {
            output.push('/');
        } else {
            output.push(ru_keyboard_char(ch).unwrap_or(ch).to_ascii_lowercase());
        }
    }
    output
}

fn ru_keyboard_char(ch: char) -> Option<char> {
    Some(match ch {
        'ё' | 'Ё' => '`',
        'й' | 'Й' => 'q',
        'ц' | 'Ц' => 'w',
        'у' | 'У' => 'e',
        'к' | 'К' => 'r',
        'е' | 'Е' => 't',
        'н' | 'Н' => 'y',
        'г' | 'Г' => 'u',
        'ш' | 'Ш' => 'i',
        'щ' | 'Щ' => 'o',
        'з' | 'З' => 'p',
        'х' | 'Х' => '[',
        'ъ' | 'Ъ' => ']',
        'ф' | 'Ф' => 'a',
        'ы' | 'Ы' => 's',
        'в' | 'В' => 'd',
        'а' | 'А' => 'f',
        'п' | 'П' => 'g',
        'р' | 'Р' => 'h',
        'о' | 'О' => 'j',
        'л' | 'Л' => 'k',
        'д' | 'Д' => 'l',
        'ж' | 'Ж' => ';',
        'э' | 'Э' => '\'',
        'я' | 'Я' => 'z',
        'ч' | 'Ч' => 'x',
        'с' | 'С' => 'c',
        'м' | 'М' => 'v',
        'и' | 'И' => 'b',
        'т' | 'Т' => 'n',
        'ь' | 'Ь' => 'm',
        'б' | 'Б' => ',',
        'ю' | 'Ю' => '.',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_russian_layout_palette_query() {
        assert_eq!(
            normalized_command_query(".уаащке").as_deref(),
            Some("/effort")
        );
        assert_eq!(normalized_command_query("/ьщву сщвуч-щтдн"), None);
        assert_eq!(normalized_command_query("/plan "), None);
        assert_eq!(normalized_command_query("/plan задача"), None);
    }

    #[test]
    fn normalizes_known_command_without_touching_plan_body() {
        assert_eq!(
            normalize_command_line_for_execution(".здфт Привет").as_deref(),
            Some("/plan Привет")
        );
    }

    #[test]
    fn normalizes_known_command_arguments() {
        assert_eq!(
            normalize_command_line_for_execution(".ьщву сщвуч-щтдн").as_deref(),
            Some("/mode codex-only")
        );
        assert_eq!(
            normalize_command_line_for_execution(".ыуеештпы").as_deref(),
            Some("/settings")
        );
        assert_eq!(
            normalize_command_line_for_execution(".срфе-ьщвуд сдфгву").as_deref(),
            Some("/chat-model claude")
        );
        assert_eq!(
            normalize_command_line_for_execution(".еруьу кщыу").as_deref(),
            Some("/theme rose")
        );
        assert_eq!(
            normalize_command_line_for_execution(".сщдщк фьиук").as_deref(),
            Some("/color amber")
        );
        assert_eq!(
            normalize_command_line_for_execution(".кщдуы сщвуч сдфгву").as_deref(),
            Some("/roles codex claude")
        );
        assert_eq!(normalized_plain_command("дщпщге"), "logout");
    }
}
