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
