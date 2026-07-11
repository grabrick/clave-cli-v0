# Clave

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust 2021](https://img.shields.io/badge/rust-2021-orange.svg)](Cargo.toml)
[![Install](https://img.shields.io/badge/install-cargo%20install-green.svg)](#install)

**Clave is a local Rust TUI that lets Claude Code and Codex CLI work together as
coding agents.** It gives you one terminal interface for discussion, planning,
implementation, review, and multi-agent handoffs without storing provider
credentials.

Clave calls the `claude` and `codex` CLIs that are already installed and logged in
on your machine. The tools keep using their own local auth. Clave only
orchestrates them around your working directory.

```bash
cargo install --git https://github.com/grabrick/clave-cli
clave
```

## Why Clave

- **One terminal for both agents.** Switch between Claude and Codex without
  rebuilding your workflow around a provider-specific UI.
- **Tandem mode.** One model acts as executor, the other as critic: they discuss
  the approach, implement, review the real result, then revise.
- **Plan gate.** Ask for a plan first, inspect it, then press `Enter` to execute
  or add feedback to revise the plan.
- **Local-first auth.** No provider sessions, API keys, or credentials are stored
  by Clave.
- **Native terminal feel.** Conversation history is rendered into the terminal
  scrollback, so mouse selection and wheel scrolling stay natural.

## Install

### Quick Install (prebuilt binary, no Rust needed)

macOS / Linux:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/grabrick/clave-cli/releases/latest/download/clave-installer.sh | sh
```

Windows (PowerShell):

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/grabrick/clave-cli/releases/latest/download/clave-installer.ps1 | iex"
```

Or download a prebuilt archive for your platform from the
[Releases page](https://github.com/grabrick/clave-cli/releases) and put the
`clave` binary anywhere on your `PATH`.

> **macOS note:** the binaries are not yet code-signed or notarized. On first run,
> Gatekeeper may block `clave` as coming from an "unidentified developer". Allow it
> via *System Settings → Privacy & Security → Open Anyway*, or run
> `xattr -d com.apple.quarantine "$(which clave)"`.

### Requirements

- Claude Code CLI as `claude`, installed and logged in
- Codex CLI as `codex`, installed and logged in
- On Windows, `/plan` needs WSL or Git Bash (the planning engine is a Bash script);
  direct chat works natively

### From Cargo

```bash
cargo install --git https://github.com/grabrick/clave-cli
```

This installs the `clave` binary into `~/.cargo/bin`. If `clave` is then reported as
"command not found", that directory is not on your `PATH` — common when `cargo` was
installed via Homebrew or a system package (rustup adds it automatically). Add it:

```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc && source ~/.zshrc
```

The planning engine is embedded into the binary and unpacked on first use into
`~/.clave/engine/`, so the TUI and `/plan` work without keeping the repository next
to the executable.

### From Source

```bash
git clone https://github.com/grabrick/clave-cli
cd clave-cli
cargo build --release
./clave
```

The `./clave` launcher looks for the release binary and can fall back to a local
Cargo run during development.

## Quick Start

```bash
clave                  # open the interactive TUI
clave "<task>"         # run a task directly through the planning engine
clave --help
```

On first launch, Clave checks whether `codex` and `claude` are available and
logged in. It can guide you through login and writes the initial config to
`~/.clave/config`.

## Modes

Use `Shift+Tab` to cycle direct-chat modes.

| Mode | What it does | File access |
|------|--------------|-------------|
| **Discussion** | answers without tools | none |
| **Plan** | drafts a plan, waits for approval, then executes | read-only plan phase, full execution phase |
| **Full Access** | autonomously reads, edits, and runs shell commands | read / write / Bash |
| **Tandem** | executor + critic workflow with two models | two agents working together |

In Plan mode, when a plan is shown:

- press `Enter` to execute it;
- type feedback and press `Enter` to revise it;
- press `Esc` to cancel.

Tandem uses the current provider roles. For example, `claude-codex` means Claude
executes and Codex reviews; `codex-claude` flips the roles.

## Planning Engine

`/plan <task>` inside the TUI, or `clave "<task>"` from the shell, runs the
embedded `spec-clave` engine:

1. the architect writes an implementation spec;
2. the reviewer returns `VERDICT: APPROVE` or `VERDICT: REVISE`;
3. the architect revises when needed;
4. Clave stops after approval or after the configured round limit.

Run the engine directly from a checkout:

```bash
./spec-clave "<task>"
./spec-clave --dry-run "<task>"
./spec-clave --rounds 3 --out .clave "<task>"
./spec-clave --architect claude --reviewer codex "<task>"
./spec-clave --codex-only --effort xhigh "<task>"
```

`--dry-run` is the cheapest way to inspect orchestration without calling either
provider.

## Commands

Type `/` to open the command palette. Russian keyboard layout is normalized for
commands, so `.здфт` becomes `/plan`.

Common commands:

| Command | Purpose |
|---------|---------|
| `/plan <task>` | run the multi-agent planning loop |
| `/brainstorm` | explore options before implementation |
| `/blueprint` | turn context into a step-by-step plan |
| `/autofix-pr` | monitor and fix issues in the current PR |
| `/chat-model codex\|claude` | choose the direct-chat provider |
| `/mode codex-only\|claude-codex\|codex-claude\|claude-only` | choose architect/reviewer roles |
| `/roles <executor> <reviewer>` | set planning roles directly |
| `/effort` | adjust reasoning effort |
| `/settings` | open model, theme, role, round, and language settings |
| `/theme purple\|cyan\|rose\|amber\|mono` | change terminal palette |
| `/lang ru\|en` | switch interface language |
| `/new`, `/chats`, `/resume <id>` | manage saved chats |
| `/export` | export the current chat to Markdown |
| `/search` | search the transcript |
| `/cost` | show model usage and estimated cost |
| `/uptime` | show how long the session has been running |
| `/help` | show the full command list |

## Shortcuts

`Shift+Tab` mode · `Enter` send · `Ctrl+J` newline · `Tab` autocomplete ·
`Up/Down` input history · `Ctrl+R` search · `Ctrl+A/E` start/end ·
`Ctrl+W/U/K` delete · `Alt+Left/Right` jump by word · `Esc` reset ·
`Ctrl+C` twice to exit · `?` controls panel.

Terminal history belongs to the native scrollback buffer. Use the mouse wheel to
scroll and drag to select text.

## Security Model

- Clave does **not** store provider credentials.
- It calls your local `claude` and `codex` CLIs, which keep using their own auth.
- Agents operate in the configured working directory.
- Claude is started with `--strict-mcp-config`, so it only receives the tools for
  the active mode instead of inheriting global MCP servers.
- Full Access, Tandem, and `/plan` can execute code. Review plans and run Clave
  only inside repositories you trust.

## State And Environment

State lives in `~/.clave/` by default:

- `config`
- `history`
- `chats/`
- unpacked planning engine files

Useful environment variables:

| Variable | Purpose |
|----------|---------|
| `CLAVE_HOME` | override the state directory |
| `CLAVE_CONFIG` | override the config file path |
| `CLAVE_CLAUDE` / `CLAVE_CODEX` | override CLI binary paths, useful for tests |
| `CLAVE_ENGINE` | use a custom `spec-clave` engine path |
| `CLAVE_SKIP_ONBOARDING=1` | skip the first-run wizard |

## Architecture

Clave has two layers:

- **`clave`**: the Rust TUI, built with `crossterm` and `ratatui`, without an async
  runtime. Long-running work uses threads and channels.
- **`spec-clave`**: the Bash planning engine that coordinates architect/reviewer
  rounds and writes artifacts.

Main Rust areas:

- `src/model/` - core types and constants: `Mode`, `Provider`, `Theme`,
  `Language`, `ChatMode`, `RunAccess`, effort tables, commands, shortcuts.
- `src/app/` - state and behavior: chat, planning, tandem, settings, saved chats,
  events, footer.
- `src/ui/` - `ratatui` rendering that reads `&App`.
- `src/runtime.rs` - entrypoint, event loop, key handling.
- `src/worker.rs` - provider and engine execution, stream parsing.
- `src/storage.rs`, `src/auth.rs`, `src/input.rs` - persistence, CLI auth checks,
  input helpers.

## Development

```bash
cargo build --release
cargo run
cargo test
cargo fmt
cargo clippy
```

Cheap orchestration check:

```bash
./spec-clave --dry-run "<task>"
```

Inline-render check:

```bash
python3 scripts/render_check.py target/release/clave <CLAVE_HOME>
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for development conventions.

## Project Status

Clave is an early `0.1.0` CLI-first tool built for local agent workflows. The
core TUI, direct chat, planning loop, saved chats, and settings exist today.

Near-term roadmap:

- release CI for macOS, Linux, and Windows binaries;
- Homebrew formula;
- public demo clip / asciinema;
- stronger docs for Tandem workflows.

## Русская версия

**Clave** - локальный Rust TUI-оркестратор, который связывает Claude Code CLI
(`claude`) и Codex CLI (`codex`) в один рабочий интерфейс. Обе модели могут
работать как агенты: читать проект, править файлы, выполнять команды, планировать
и ревьюить результат.

Креды провайдеров не хранятся. Clave вызывает уже залогиненные локальные CLI
пользователя, поэтому Claude и Codex продолжают использовать свои собственные
локальные сессии.

Быстрая установка:

```bash
cargo install --git https://github.com/grabrick/clave-cli
clave
```

Основные режимы:

- **Discussion** - простой чат без инструментов.
- **Plan** - сначала план, потом выполнение по подтверждению.
- **Full Access** - агент сам читает, правит и запускает команды.
- **Tandem** - исполнитель и критик: обсуждение, исполнение, ревью, финальная
  правка.

Планирование:

```bash
/plan <задача>
```

или напрямую из shell:

```bash
clave "<задача>"
```

Артефакты планирования пишутся в каталог запуска, обычно в `.clave/`, а итоговый
brief возвращается обратно в чат.

Интерфейс по умолчанию русский, переключение языка доступно через `/lang ru|en`.
