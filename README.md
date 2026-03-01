# ralph-cli

A Rust TUI for managing and running [Ralph](https://github.com/anthropics/claude-code) agent loops. Replaces the `scripts/ralph/ralph.sh` and `scripts/ralph/ralph-status.sh` bash scripts with a live terminal dashboard.

> **Status:** In active development. The TUI opens, displays plans and story progress, and supports creating new plans. Loop execution ([r]un/[s]top), editing, deletion, and the help overlay are not yet implemented. The shell scripts in `scripts/ralph/` remain the working implementation for running loops.

---

## What it does

Run `ralph-cli` inside any git repository to:

- Browse all plans stored in `.ralph/plans/`
- See per-plan story progress at a glance
- Start and stop Ralph loops (spawns `claude --agent ralph` under the hood)
- Stream subprocess output live into a log panel
- Create, edit, and delete plans without leaving the terminal

The three-panel layout mirrors the information you get from `ralph-status.sh` but stays live and interactive.

---

## Plan file layout

Plans live inside the repository under `.ralph/plans/`:

```
.ralph/
└── plans/
    └── <plan-name>/
        ├── prd.json      # stories, validation commands, branch name
        └── progress.txt  # append-only implementation log
```

### prd.json schema

```json
{
  "project": "my-project",
  "branchName": "my-feature-branch",
  "description": "What this plan delivers",
  "validationCommands": ["cargo build", "cargo clippy -- -D warnings"],
  "userStories": [
    {
      "id": "US-001",
      "title": "Short title",
      "description": "As a ..., I need ...",
      "acceptanceCriteria": ["criterion one", "criterion two"],
      "priority": 1,
      "passes": false,
      "notes": ""
    }
  ]
}
```

`passes: true` means the story has been implemented and all validation commands passed. The Ralph agent sets this itself before committing.

## Keybindings

| Key | Action |
|---|---|
| `j` / `↓` | Move focus down in the plans list |
| `k` / `↑` | Move focus up in the plans list |
| `n` | Open "New plan" dialog |
| `q` | Quit |

Bindings for `r` (run), `s` (stop), `e` (edit), `d` (delete), and `?` (help) are shown in the status bar but are not yet implemented.

---

## Development

### Prerequisites

- Rust (edition 2024, toolchain ≥ 1.93)
- [`just`](https://github.com/casey/just) task runner (`cargo install just` or `brew install just`)

### Common tasks

```sh
just build        # cargo build
just check        # build + clippy (the full validation gate)
just lint         # cargo clippy -- -D warnings
just test         # cargo test
just run          # cargo run
just fmt          # cargo fmt
just fmt-check    # check formatting without modifying files
just fix          # cargo clippy --fix --allow-staged
just clean        # cargo clean
just              # list all recipes
```

`just check` runs the same commands as `prd.json`'s `validationCommands`.

### Project structure

```
src/
├── main.rs            # entry point, module imports
├── app.rs             # App state struct (TUI state machine)
├── ui.rs              # ratatui draw function
└── ralph/
    ├── mod.rs         # module declarations
    ├── store.rs       # Store — git root detection, .ralph/plans/ management
    ├── plan.rs        # Plan, PrdJson, UserStory — prd.json I/O
    └── runner.rs      # Ralph subprocess runner (stub)
```

**`store.rs`** — `Store::find(path)` walks up from any path to find the git root. All plan directory paths go through `Store` methods.

**`plan.rs`** — `Plan::load(dir)` deserializes `prd.json`. `Plan::save(dir)` writes it back. Helper methods: `done_count()`, `total_count()`, `next_story()`, `is_complete()`.

### Key dependencies

| Crate | Purpose |
|---|---|
| `ratatui` | Terminal UI framework |
| `crossterm` | Cross-platform terminal backend |
| `serde` + `serde_json` | prd.json serialization |
| `anyhow` | Error handling |
| `tokio` | Async runtime for subprocess streaming |
| `clap` | CLI argument parsing |

---

## Legacy shell scripts

The original implementation lives in `scripts/ralph/` and still works:

```sh
# Run the ralph loop (interactive, prompts between stories)
./scripts/ralph/ralph.sh [max_iterations]
./scripts/ralph/ralph.sh -f path/to/prd.json [max_iterations]

# Print story progress for the current prd.json
./scripts/ralph/ralph-status.sh
```

**Prerequisites for the shell scripts:** `claude` CLI and `jq` must be on `PATH`.

The shell scripts expect `prd.json` in the current working directory. They archive previous runs to `scripts/ralph/archive/` when the `branchName` changes.

### Migration

If you have an existing project using the `scripts/ralph/prd.json` layout, `ralph-cli` will detect it on startup and offer to migrate to `.ralph/plans/` automatically (once the TUI is complete — US-014).

---

## Running the Ralph agent

The Ralph agent prompt and instructions live in `.ralph/` (agent config) and are loaded by `claude --agent ralph`. The agent reads the highest-priority incomplete story from `prd.json`, implements it, runs validation, and sets `passes: true` before committing.

Each invocation handles exactly one story. The loop (either `ralph-cli` or `ralph.sh`) re-invokes the agent until all stories are complete or it receives `<promise>COMPLETE</promise>` in the output.
