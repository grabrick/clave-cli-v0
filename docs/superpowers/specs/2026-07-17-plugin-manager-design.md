# Плагин-менеджер clave — дизайн

**Дата:** 2026-07-17 · **Статус:** дизайн одобрен, реализация по фазам.

## Цель
Единая панель `/plugins` для управления плагинами **обоих** провайдеров (Claude Code + Codex)
из clave — чтобы не заходить в каждый инструмент вручную. Полный магазин: просмотр, поиск,
установка/удаление, вкл/выкл, обновление, управление marketplace-источниками.

## Реальность CLI (выверено)
| Действие | Claude | Codex |
|---|---|---|
| Список | `plugin list` + `~/.claude/plugins/{installed_plugins,plugin-catalog-cache}.json` | `plugin list --available --json` |
| Установить | `plugin install <p@m>` | `plugin add <p@m>` |
| Удалить | `plugin uninstall <p>` | `plugin remove <p@m>` |
| Вкл/выкл | `plugin enable/disable <p>` | features `-c features.<name>=true/false` |
| Обновить | `plugin update <p>` | (через marketplace upgrade) |
| Marketplace | `plugin marketplace add/list/remove` | `plugin marketplace add/list/remove` |

Симметрично, кроме вкл/выкл (claude — команда, codex — features-конфиг) — прячется в бэкенде.

## Архитектура
- **`PluginBackend`** (trait, `plugins/mod.rs`) + реализации `ClaudePlugins`, `CodexPlugins`.
  Бэкенд — **чистая логика без спавна**: (1) парсит вывод/JSON/конфиг → `PluginEntry`;
  (2) строит `Command` для действия. Спавн и I/O — в слое App/worker.
- **Гибрид источника (подход C):** список codex — `plugin list --available --json`; claude —
  `plugin list` + `catalog-cache.json`. **Действия — всегда через CLI** (никогда не пишем в чужие
  конфиги руками). Browse быстрый, действия официальные/безопасные.
- **Конфиг-путь ИНЪЕКТИРУЕТСЯ** в бэкенд (не хардкод `~/.claude`/`~/.codex`) — тесты на temp-фикстурах,
  НИКОГДА не трогают реальные каталоги (прямой урок BUG-006).

## Модель
`PluginEntry { provider: Provider, name, marketplace, installed: bool, enabled: bool, version: Option<String> }` —
общая для обоих; секции панели фильтруют по `provider`.

## Компоненты
- `model/plugin.rs` — `PluginEntry`, `Marketplace`.
- `plugins/mod.rs` + `plugins/claude.rs` + `plugins/codex.rs` — трейт и реализации.
- `app/plugins.rs` — App-методы: открыть панель, диспетч действий (спавн через `run_hooks.spawn` —
  переиспользуем раннер-seam), приём результатов.
- `ui/plugins.rs` — рендер панели (раздельные секции Claude/Codex).
- `Overlay::Plugins` + команда `/plugins` (по образцу `/chats`).

## Поток данных
1. `/plugins` → `Overlay::Plugins` + загрузка: claude синхронно (конфиг), codex асинхронно
   (worker, `--json`) → `WorkerEvent::PluginsLoaded`. Пока codex грузится — «загрузка…».
2. Отображение: раздельные секции, статус `●`уст/`○`дост, вкл/выкл, версия. Навигация `↑↓`.
3. Поиск `/` — инкрементальный фильтр по имени.
4. Действие → спавн CLI → «устанавливаю…» → `PluginActionDone` → refresh.

## Клавиши и подтверждения
| Клавиша | На чём | Действие | Подтверждение |
|---|---|---|---|
| `Enter` | доступный | установить | да (меняет окружение) |
| `Enter` | установленный | удалить | да |
| `e` | установленный | вкл/выкл | нет (обратимо) |
| `u` | установленный | обновить | нет |
| `m` | — | режим marketplace (add/remove) | add — да |

Подтверждение — строка внизу панели (`⚠ Установить X из Y? Enter — да · Esc — отмена`), не отдельный overlay.

## Состояние App (новые поля)
`plugins: Vec<PluginEntry>`, `plugins_index`, `plugins_query`, `plugins_loading`,
`plugins_confirm: Option<PendingPluginAction>`, `plugins_marketplace_mode: bool`.

## Обработка ошибок (панель не рушится, объясняет)
- CLI-действие exit≠0 → причина из stderr строкой у плагина (логика `chat_error_lines`).
- Провайдер не в PATH → его секция «claude/codex CLI не найден», вторая работает.
- codex `--json` битый/пустой → «не удалось загрузить список» (без паники, lossy).
- claude каталог-кэш отсутствует → установленные + пометка «доступные не загружены».
- Долгое действие (git clone) → отменяемо (`cancel_rx`).

## Тестирование
- Бэкенд читает конфиг из инъектируемого пути → temp-фикстуры, не реальный `~/.claude`.
- Парсинг: реальные примеры JSON как фикстуры → `PluginEntry`.
- Сборка команд: `install_cmd("context7@mkt")` → верные args (claude vs codex), без спавна.
- Диспетч: фейковый `spawn` (noop) → `Enter`/`e` спавнят верную команду верного провайдера.
- Рендер: TestBackend — секции, статусы, фильтр, строка подтверждения.

## Фазы реализации (каждая — отгружаемый инкремент по циклу develop→cherry-pick→push)
- **Фаза 1 — Просмотр:** модели + бэкенд-`list` + `Overlay::Plugins` + `/plugins` + рендер. Только чтение.
- **Фаза 2 — Действия:** install/uninstall/enable/disable/update + подтверждения + refresh + поиск.
- **Фаза 3 — Marketplace:** режим add/remove источников.
