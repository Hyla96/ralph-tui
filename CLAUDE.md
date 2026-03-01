# ralph-cli

## Build and development commands

Use `just` as the task runner. Run `just` with no arguments to list all recipes.

| Recipe | Command |
|---|---|
| `just build` | `cargo build` |
| `just release` | `cargo build --release` |
| `just check` | build + clippy (matches prd.json validationCommands) |
| `just lint` | `cargo clippy -- -D warnings` |
| `just test` | `cargo test` |
| `just run` | `cargo run` |
| `just fmt` | `cargo fmt` |
| `just fmt-check` | `cargo fmt -- --check` |
| `just fix` | `cargo clippy --fix --allow-staged` |
| `just clean` | `cargo clean` |

The PRD validation commands (`cargo build` and `cargo clippy -- -D warnings`) are covered by `just check`.

## Codebase notes

- Rust edition 2024, toolchain 1.93.1.
- `#![allow(dead_code)]` in main.rs suppresses lint for scaffold stubs during early stories; remove as modules are wired together.
- Plans live in `.ralph/plans/<name>/prd.json`. Store and Plan in `src/ralph/` manage all access.
- serde field renames are per-field (`#[serde(rename = "...")]`) to match the existing camelCase JSON schema.
- `main()` uses `#[tokio::main]` (multi-threaded runtime); `tokio::spawn` is callable from any sync function transitively called from main. `event::poll(100ms)` in the main loop is acceptable — short blocking, worker threads handle spawned tasks.
- `clippy::let_underscore_future`: use `drop(tokio::spawn(...))` not `let _ = tokio::spawn(...)` for fire-and-forget tasks.
- Runner channel drain pattern: collect into a local `Vec` inside the borrow scope, process after releasing the borrow, to avoid simultaneous mutable field borrows.
- `vt100::Parser::new(rows, cols, scrollback_len)` — rows before cols. No resize() method; recreate the parser on resize (screen state is lost).
- tui-term 0.3.1 requires ratatui 0.30+ (via unicode-width conflict). Always use ratatui ≥ 0.30 with tui-term.
- Scrollback position lives on the vt100 Screen (`screen_mut().set_scrollback(n)`). Update it in event handlers rather than in draw (draw takes `&App`) so `PseudoTerminal` renders the correct view without needing `&mut App`.
