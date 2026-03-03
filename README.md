# ralph-tui

A Rust TUI for managing and running [Ralph](https://ghuntley.com/ralph/) agent loops with [Claude Code](https://github.com/anthropics/claude-code).

---

## What it does

Run `ralph-tui` inside any git repository to:

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

| Key            | Action                                         |
| -------------- | ---------------------------------------------- |
| `j` / `↓`      | Move selection down                            |
| `k` / `↑`      | Move selection up                              |
| `r`            | Run selected workflow (opens a new runner tab) |
| `s`            | Stop the runner for the selected workflow      |
| `n`            | Open "New workflow" dialog                     |
| `e`            | Edit `prd.json` in `$EDITOR`                   |
| `d`            | Delete selected workflow (with confirmation)   |
| `?`            | Open help overlay                              |
| `t` + chord    | Navigate tabs (see below)                      |
| `q` / `Ctrl+C` | Quit                                           |

### Runner tab

| Key            | Action                                          |
| -------------- | ----------------------------------------------- |
| `k` / `↑`      | Scroll log up                                   |
| `j` / `↓`      | Scroll log down                                 |
| `G` / `End`    | Jump to bottom (resume auto-scroll)             |
| `s`            | Stop the runner                                 |
| `x`            | Close tab (only when runner is done or errored) |
| `Enter`        | Send input buffer to subprocess stdin           |
| `Esc`          | Clear input buffer without sending              |
| `t` + chord    | Navigate tabs (see below)                       |
| `q` / `Ctrl+C` | Quit                                            |

### Tab navigation (`t` chord)

Press `t`, then:

| Key       | Action                                    |
| --------- | ----------------------------------------- |
| `1`–`9`   | Jump to tab by number (1 = Workflows tab) |
| `←` / `→` | Cycle through tabs with wrapping          |


---

## Getting Started

ralph-tui is a Rust TUI that manages the Ralph agent loop. Ralph is a Claude Code agent that reads a `prd.json` workflow file, implements one task at a time, runs validation, and commits the result. ralph-tui is the controller: it discovers workflows in a git repo, lets you start and stop agent loops, and streams the live output. You bring the repo and the task spec; ralph-tui and Ralph take care of the rest.

### Step 1 — Prerequisites

- **Rust** (edition 2024, toolchain ≥ 1.86). Install via [rustup](https://rustup.rs/):
  ```sh
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **[`just`](https://github.com/casey/just)** task runner:
  ```sh
  cargo install just
  # or on macOS:
  brew install just
  ```

### Step 2 — Clone the repository

```sh
git clone https://github.com/your-org/ralph-tui.git
cd ralph-tui
```

If you are reviewing a pull request, check out the relevant branch:

```sh
git checkout <branch-name>
```

### Step 3 — Copy agent resources

```sh
just set-resources
```

This copies the `/prd` and `/prd-synth` skills and the `ralph` agent into `~/.claude/`, making them available to Claude Code.

### Step 4 — Install the binary

```sh
just install
```

This runs `cargo install --path .` and installs the `ralph-tui` binary on your `PATH`.

### Step 5 — Create a sandbox repository

> **Note:** ralph-tui must always be run from inside a git repository.

```sh
mkdir my-project && cd my-project && git init && git commit --allow-empty -m 'init'
```

### Step 6 — Generate a workflow with `/prd`

Open a Claude Code session inside your sandbox repo and run the `/prd` skill to describe the feature you want to build:

```sh
claude
```

Inside the Claude Code session:

```
/prd We need a feature that shows token usage for ralph usage in this TUI
```

Claude will walk you through refining the spec.

### Step 7 — Synthesize `prd.json` with `/prd-synth`

In the same Claude Code session, run:

```
/prd-synth
```

This emits `.ralph/workflows/<name>/prd.json` containing the structured task list.

### Step 8 — Exit Claude and launch ralph-tui

Exit the Claude Code session, then from the repo root:

```sh
ralph-tui
```

### Step 9 — Start the agent loop

In the Workflows tab, select your workflow and press `r` to start the agent loop. Ralph will implement tasks one by one. Wait for each task to complete; the runner tab streams live output.

For a full reference of key bindings, see [Keybindings](#keybindings).

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

| Crate                  | Purpose                                |
| ---------------------- | -------------------------------------- |
| `ratatui`              | Terminal UI framework                  |
| `crossterm`            | Cross-platform terminal backend        |
| `serde` + `serde_json` | prd.json serialization                 |
| `anyhow`               | Error handling                         |
| `tokio`                | Async runtime for subprocess streaming |
| `clap`                 | CLI argument parsing                   |

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
