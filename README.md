# ralph-cli

A Rust TUI for managing and running [Ralph](https://github.com/anthropics/claude-code) agent loops. Replaces the `scripts/ralph/ralph.sh` and `scripts/ralph/ralph-status.sh` bash scripts with a live terminal dashboard.

---

## What it does

Run `ralph-cli` inside any git repository to:

- Browse all workflows stored in `.ralph/workflows/`
- See per-workflow task progress at a glance
- Run and stop Ralph loops — spawns `claude --agent ralph` in a live runner tab
- Stream subprocess output and send stdin input without leaving the terminal
- Create, edit (via `$EDITOR`), and delete workflows

Each running workflow opens in its own tab. Multiple workflows can run concurrently.

---

## Workflow file layout

Workflows live inside the repository under `.ralph/workflows/`:

```
.ralph/
└── workflows/
    └── <workflow-name>/
        └── prd.json      # tasks, validation commands, branch name
```

### prd.json schema

```json
{
  "project": "my-project",
  "branchName": "my-feature-branch",
  "description": "What this workflow delivers",
  "validationCommands": ["cargo build", "cargo clippy -- -D warnings"],
  "tasks": [
    {
      "id": "TASK-001",
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

`passes: true` means the task has been implemented and all validation commands passed. The Ralph agent sets this itself before committing.

---

## Keybindings

### Workflows tab

| Key | Action |
|---|---|
| `j` / `↓` | Move selection down |
| `k` / `↑` | Move selection up |
| `r` | Run selected workflow (opens a new runner tab) |
| `s` | Stop the runner for the selected workflow |
| `n` | Open "New workflow" dialog |
| `e` | Edit `prd.json` in `$EDITOR` |
| `d` | Delete selected workflow (with confirmation) |
| `?` | Open help overlay |
| `t` + chord | Navigate tabs (see below) |
| `q` / `Ctrl+C` | Quit |

### Runner tab

| Key | Action |
|---|---|
| `k` / `↑` | Scroll log up |
| `j` / `↓` | Scroll log down |
| `G` / `End` | Jump to bottom (resume auto-scroll) |
| `s` | Stop the runner |
| `x` | Close tab (only when runner is done or errored) |
| `Enter` | Send input buffer to subprocess stdin |
| `Esc` | Clear input buffer without sending |
| `t` + chord | Navigate tabs (see below) |
| `q` / `Ctrl+C` | Quit |

### Tab navigation (`t` chord)

Press `t`, then:

| Key | Action |
|---|---|
| `1`–`9` | Jump to tab by number (1 = Workflows tab) |
| `←` / `→` | Cycle through tabs with wrapping |

---

## Development

### Prerequisites

- Rust (edition 2024, toolchain ≥ 1.86)
- [`just`](https://github.com/casey/just) task runner (`cargo install just` or `brew install just`)

### Common tasks

```sh
just build        # cargo build
just check        # build + clippy (the full validation gate)
just lint         # cargo clippy -- -D warnings
just test         # cargo test
just run          # cargo run
just run-log      # cargo run with stderr → /tmp/ralph.log (for debugging)
just fmt          # cargo fmt
just fmt-check    # check formatting without modifying files
just fix          # cargo clippy --fix --allow-staged
just clean        # cargo clean
just              # list all recipes
```

`just check` runs the same commands as `prd.json`'s `validationCommands`.

To watch logs while debugging:

```sh
# Terminal 1
just run-log

# Terminal 2
tail -f /tmp/ralph.log
```

### Project structure

```
src/
├── main.rs            # entry point, panic hook, ratatui init
├── app.rs             # App state struct, event loop, keybindings
├── ui.rs              # ratatui draw functions
└── ralph/
    ├── mod.rs         # module declarations
    ├── store.rs       # Store — git root detection, .ralph/workflows/ management
    ├── workflow.rs    # Workflow, PrdJson, Task — prd.json I/O
    └── runner.rs      # RunnerEvent — event types for subprocess streaming
```

**`store.rs`** — `Store::find(path)` walks up from any path to find the git root. All workflow directory paths go through `Store` methods.

**`workflow.rs`** — `Workflow::load(dir)` deserializes `prd.json`. `Workflow::save(dir)` writes it back. Helper methods: `done_count()`, `total_count()`, `next_task()`, `is_complete()`.

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

## Running the Ralph agent

The Ralph agent prompt and instructions live in `.claude/` and are loaded by `claude --agent ralph`. The agent reads the highest-priority incomplete task from `prd.json`, implements it, runs validation, and sets `passes: true` before committing.

Each invocation handles exactly one task. The runner loop re-invokes the agent until all tasks are complete or it receives `<promise>COMPLETE</promise>` in the output.

---

## Legacy shell scripts

The original implementation lives in `scripts/ralph/` and still works:

```sh
# Run the ralph loop (interactive, prompts between tasks)
./scripts/ralph/ralph.sh [max_iterations]
./scripts/ralph/ralph.sh -f path/to/prd.json [max_iterations]

# Print task progress for the current prd.json
./scripts/ralph/ralph-status.sh
```

**Prerequisites for the shell scripts:** `claude` CLI and `jq` must be on `PATH`.
