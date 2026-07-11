# Contributing to Clave

Thanks for taking a look at Clave. This project is still early, so the most
useful contributions are small, concrete, and easy to review: install friction,
terminal rendering issues, provider invocation bugs, documentation fixes, and
focused improvements to the planning or Tandem flows.

## Development Setup

Requirements:

- Rust stable + `cargo`
- Claude Code CLI as `claude`, installed and logged in for real provider runs
- Codex CLI as `codex`, installed and logged in for real provider runs
- Python 3 + `pyte` for terminal render checks

Install the optional render-check dependency:

```bash
pip3 install pyte
```

For unit tests, real provider CLIs can be replaced with mocks by setting
`CLAVE_CLAUDE` and `CLAVE_CODEX`.

## Build And Checks

```bash
cargo build --release        # builds target/release/clave
cargo test                   # unit tests
cargo fmt                    # standard Rust formatting
cargo clippy                 # lint
```

Cheap orchestration check without spending provider tokens:

```bash
./spec-clave --dry-run "<task>"
```

Terminal inline-render check:

```bash
python3 scripts/render_check.py target/release/clave <CLAVE_HOME>
```

`CLAVE_HOME` should point to a temporary directory containing `config` and
`chats/`; see the script header for the exact setup.

## Architecture Summary

Clave has two layers:

- `spec-clave` - a Bash architect/reviewer planning engine.
- `src/` - the Rust TUI, direct chat runtime, settings, and storage.

Important Rust areas:

- `model/` - pure types and constants: `Mode`, `Provider`, `Theme`, `Language`,
  `ChatMode`, commands, shortcuts, effort tables.
- `app/` - state and behavior. `App` is declared in `app/mod.rs`, while methods
  live across `app/*.rs`.
- `ui/` - rendering only. UI code reads `&App` and should not mutate state.
- `worker.rs` - provider process execution and stream parsing.
- `storage.rs`, `auth.rs`, `input.rs` - persistence, CLI auth, input helpers.

The project intentionally avoids an async runtime. Long-running work uses
threads and `mpsc` channels; the main loop polls events and keyboard input.

## Code Conventions

- The UI defaults to Russian. User-visible strings should go through
  `lang.choose("ru", "en")`; avoid hard-coded visible UI text.
- Technical identifiers, filenames, command names, and commit messages should be
  in English.
- Keep dependencies minimal and justify any new dependency in the PR.
- Keep changes scoped. Avoid broad refactors unless they are necessary for the
  behavior being changed.

## Adding A Slash Command

Slash commands need three coordinated changes:

1. Add a `CommandSpec` entry in `COMMANDS` in `src/model/commands.rs`.
2. Add the handling branch in `App::handle_command` in `src/app/commands.rs`.
3. Add the command token to `App::command_has_handler` in
   `src/app/commands.rs` under `#[cfg(test)]`.

The palette test checks that every command has a handler.

## Before Opening A PR

Run:

```bash
cargo fmt
cargo clippy
cargo test
```

If you changed rendering, panels, transcript layout, or terminal behavior, also
run:

```bash
python3 scripts/render_check.py target/release/clave <CLAVE_HOME>
```

Commit messages should be short, English, and imperative, for example:

```text
Fix tandem review status
Document cargo install path
```

## Cutting A Release

Releases are built by `dist` (cargo-dist) and published to GitHub Releases by
`.github/workflows/release.yml`, triggered by a version tag. The version source of
truth is `version` in `Cargo.toml`.

```bash
# 1. Bump the version in Cargo.toml (e.g. 0.1.0 -> 0.1.1), then:
cargo build                 # refresh Cargo.lock
git commit -am "Release 0.1.1"
git push

# 2. Tag and push — this triggers the release workflow:
git tag v0.1.1
git push origin v0.1.1
```

Inspect the build matrix without spending CI minutes with `dist plan`, and build
the host artifact locally with `dist build`. The generated workflow file is managed
by dist — regenerate it with `dist generate` (or `dist init`) instead of editing it
by hand.

## Русская памятка

Рабочий язык интерфейса - русский, но публичные README/CONTRIBUTING теперь
английские, чтобы проект было проще показывать международной аудитории.

Перед PR:

```bash
cargo fmt
cargo clippy
cargo test
```

Если меняешь рендер или панели, дополнительно прогони `scripts/render_check.py`.
