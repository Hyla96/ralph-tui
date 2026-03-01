# PRD: ralph-cli Core TUI

## Introduction

`ralph-cli` is a Rust TUI application (ratatui + crossterm) that replaces the `ralph.sh` and `ralph-status.sh` bash scripts. Run `ralph-cli` in any git repository to manage plans, run ralph loops, and inspect story progress from a live terminal dashboard. State is stored in `.ralph/plans/<name>/` inside the repository.

This PRD covers the v1 foundation: project scaffold, data layer, three-panel TUI layout, plan/story navigation, basic plan management (create, edit, delete), the interactive ralph loop runner, and the migration helper for existing projects.

## Goals

- Replace `ralph.sh` and `ralph-status.sh` with a single TUI binary
- Store all state inside the repository under `.ralph/plans/`
- Stream claude subprocess output live into the log panel
- Pause between stories with a continue/cancel prompt (replicates the `[Y/n]` behavior)
- Work with the existing `prd.json` schema without changes

## User Stories

### US-001: Add Rust dependencies and scaffold module structure

**Description:** As a developer, I need Cargo.toml configured with the required crates and the source tree scaffolded with stable module paths so subsequent stories can build on them without restructuring.

**Acceptance Criteria:**

- [ ] Cargo.toml includes: `ratatui`, `crossterm`, `serde` (with derive feature), `serde_json`, `anyhow`, `tokio` (with full features), `clap` (with derive feature)
- [ ] `src/app.rs` exists (stub: empty `pub struct App`)
- [ ] `src/ui.rs` exists (stub: empty `pub fn draw` function)
- [ ] `src/ralph/mod.rs` exists and declares: `pub mod store; pub mod plan; pub mod runner;`
- [ ] `src/ralph/store.rs` exists (stub with a `Store` struct)
- [ ] `src/ralph/plan.rs` exists (stub with a `Plan` struct)
- [ ] `src/ralph/runner.rs` exists (stub)
- [ ] `main.rs` imports `app`, `ui`, `ralph` modules
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-002: Implement Store — git root detection and .ralph/plans/ management

**Description:** As a developer, I need a `Store` struct that locates the git repository root, manages the `.ralph/plans/` directory tree, and lists available plans so all other components have a reliable way to discover and access plan files.

**Acceptance Criteria:**

- [ ] `Store::find(path: &Path)` walks up the directory tree until it finds a `.git` directory and returns `Ok(Store)` with the repo root; returns `Err` if no git root found
- [ ] `Store::plans_dir()` returns `<repo_root>/.ralph/plans/` as a `PathBuf`
- [ ] `Store::list_plans()` scans `.ralph/plans/` and returns `Vec<String>` of subdirectory names that contain a `prd.json`; returns empty vec if directory does not exist
- [ ] `Store::plan_dir(name: &str)` returns `<repo_root>/.ralph/plans/<name>/` as a `PathBuf`
- [ ] `Store::create_plan(name: &str)` creates `.ralph/plans/<name>/` and writes a starter `prd.json` with empty `userStories` and the given name as `project` field; returns `Err` if plan already exists
- [ ] `Store::is_valid_name(name: &str)` returns `true` only if name matches `[a-z0-9-]{3,64}`
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-003: Implement Plan, PrdJson, and UserStory data structures with file I/O

**Description:** As a developer, I need `Plan`, `PrdJson`, and `UserStory` structs that deserialize from and serialize to `prd.json` so the TUI can read story data and ralph can continue to use the same file format.

**Acceptance Criteria:**

- [ ] `PrdJson` has fields: `project` (String), `branchName` (String), `description` (String), `validationCommands` (Vec<String>), `userStories` (Vec<UserStory>) — all serde-compatible with the existing schema
- [ ] `UserStory` has fields: `id` (String), `title` (String), `description` (String), `acceptanceCriteria` (Vec<String>), `priority` (u32), `passes` (bool), `notes` (String)
- [ ] `Plan::load(dir: &Path)` reads `prd.json` from dir and returns `Ok(Plan)`; returns `Err` if file missing or invalid JSON
- [ ] `Plan::save(&self, dir: &Path)` writes `self.prd` back to `prd.json` in dir
- [ ] `Plan::done_count(&self)` returns count of stories where `passes == true`
- [ ] `Plan::total_count(&self)` returns total number of stories
- [ ] `Plan::next_story(&self)` returns `Option<&UserStory>` — the first story where `passes == false` sorted by `priority` ascending
- [ ] `Plan::is_complete(&self)` returns `true` if all stories have `passes == true`
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-004: Bootstrap ratatui TUI with three-panel layout and quit

**Description:** As a user, I want to run `ralph-cli` and see the TUI open with the correct panel layout so I have a home base for all operations.

**Acceptance Criteria:**

- [ ] Running `ralph-cli` opens a ratatui full-screen TUI using crossterm backend in alternate screen mode with raw mode enabled
- [ ] Layout: top row split left/right (Plans panel ~25% width | Stories panel ~75% width) takes ~75% of terminal height; Log panel takes ~20%; status bar is exactly 1 line
- [ ] Each panel has a `Block` border and title: `Plans`, `Stories`, `Log`; status bar has no border
- [ ] Pressing `q` exits the TUI and restores terminal state (no leftover raw mode or alternate screen)
- [ ] A panic hook is installed via `std::panic::set_hook` that restores terminal before printing the panic message
- [ ] `main()` calls `Store::find` on the current directory; if not in a git repo, exits with error message before opening TUI
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-005: Plans panel — render plan list with keyboard navigation

**Description:** As a user, I want to see all available plans in the left panel and navigate between them with arrow keys or j/k so I can switch focus to any plan.

**Acceptance Criteria:**

- [ ] Plans panel renders a list of plan names loaded from `Store::list_plans()`
- [ ] The focused plan is highlighted using ratatui's `List` widget with a highlight style (bold or reversed)
- [ ] Arrow up, arrow down, `k`, and `j` move focus between plans; focus stops at boundaries
- [ ] If no plans exist, the panel shows: `No plans. Press [n] to create one.`
- [ ] Panel title shows plan count: e.g. `Plans (2)`
- [ ] App state holds the selected plan index; changing selection re-renders the Stories panel
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-006: Stories panel — display stories for the focused plan

**Description:** As a user, I want the right panel to show all stories for whichever plan I have focused so I can see progress at a glance without opening files.

**Acceptance Criteria:**

- [ ] Stories panel loads the `Plan` for the currently focused plan name and renders all `userStories`
- [ ] Each story row shows: checkmark `✓` or circle `○` for `passes` status, priority in brackets `[N]`, story id, and title — e.g. `✓ [1] US-001: Add schema`
- [ ] Stories where `passes == true` are rendered with a dim/grey style
- [ ] Panel title shows progress: e.g. `Stories (2/5)`
- [ ] If no plan is focused (empty list), panel shows: `Select a plan`
- [ ] Panel re-renders whenever focused plan changes
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-007: Status bar with state-aware keybinding hints

**Description:** As a user, I want a persistent bottom bar showing available keybindings and current loop state so I always know what actions are available.

**Acceptance Criteria:**

- [ ] Status bar is always visible as a 1-line `Paragraph` at the bottom of the layout
- [ ] In `Idle` state: shows `[r]un  [n]ew  [e]dit  [d]elete  [?]help  [q]uit`
- [ ] In `Running` state: shows `[s]top  [q]uit  Running iteration N/10…` where N is current iteration count
- [ ] In `Complete` state: shows `COMPLETE  [n]ew  [e]dit  [d]elete  [?]help  [q]uit`
- [ ] App state has an enum `AppState` with variants: `Idle`, `Running { iteration: u32 }`, `Complete`
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-008: New plan dialog — create a plan from the TUI

**Description:** As a user, I want to press `[n]` to open an inline input dialog that lets me name and create a new plan so I can start working without leaving the TUI.

**Acceptance Criteria:**

- [ ] Pressing `n` opens a modal overlay with a text input: `New plan name: <cursor>`
- [ ] Typing alphanumeric characters and hyphens appends to input buffer; Backspace removes the last character
- [ ] Enter confirms: calls `Store::create_plan(name)`; on success closes dialog, refreshes plan list, and focuses new plan
- [ ] Esc cancels dialog without changes
- [ ] If name fails `Store::is_valid_name`, shows inline error: `Invalid name — use lowercase letters, digits, hyphens (3–64 chars)`
- [ ] If plan already exists, shows inline error: `Plan already exists`
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-009: Edit plan — open prd.json in $EDITOR

**Description:** As a user, I want to press `[e]` on a focused plan to open its `prd.json` in my preferred editor so I can write and update stories without leaving the workflow.

**Acceptance Criteria:**

- [ ] Pressing `e` with a plan focused suspends the TUI: disable raw mode, leave alternate screen
- [ ] Spawns `$EDITOR` (falls back to `vi` if unset) with the plan's `prd.json` path as argument; waits for editor process to exit
- [ ] After editor exits, re-enables raw mode and alternate screen, re-renders TUI
- [ ] Reloads `Plan` from disk after editor closes; updated stories are immediately visible in the Stories panel
- [ ] If editor spawn fails (binary not found), shows error in status bar and does not crash
- [ ] If no plan is focused, `e` is a no-op
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-010: Delete plan with confirmation overlay

**Description:** As a user, I want to press `[d]` on a focused plan to delete it after confirmation so I can clean up finished or abandoned plans.

**Acceptance Criteria:**

- [ ] Pressing `d` with a plan focused opens a confirmation overlay: `Delete plan '<name>'? [y/N]`
- [ ] Pressing `y` deletes `.ralph/plans/<name>/` and all contents via `std::fs::remove_dir_all`
- [ ] Any other key cancels without changes and closes overlay
- [ ] Plans list refreshes after deletion; focus moves to next plan or shows empty state if none remain
- [ ] If no plan is focused, `d` is a no-op
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-011: Ralph runner — spawn claude subprocess and stream output to log panel

**Description:** As a user, I want to press `[r]` to start a ralph loop for the focused plan, with claude output streaming live into the log panel, so I can watch progress in real time.

**Acceptance Criteria:**

- [ ] Pressing `r` with a plan focused and `AppState::Idle` spawns: `claude --agent ralph "Implement the next user story."` with working directory set to repo root and env var `RALPH_PLAN_DIR=<plan-dir>`
- [ ] stdout and stderr from the subprocess are read line by line in a spawned tokio task and sent to the main task via `tokio::sync::mpsc::unbounded_channel`
- [ ] Each received line is appended to a log buffer (capped at 1000 lines rolling) and rendered in the Log panel
- [ ] Log panel auto-scrolls to the latest line during a run
- [ ] `AppState` transitions to `Running { iteration: 1 }` when subprocess starts
- [ ] Pressing `r` while already `Running` shows `Already running` in status bar for 2 seconds then clears; does not spawn a second process
- [ ] If `claude` binary is not found on PATH, shows error in status bar: `claude not found on PATH`
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-012: Ralph loop control — stop, completion detection, and story reload

**Description:** As a user, I want to stop a running loop with `[s]` and have the TUI automatically detect when ralph finishes and reload story status from disk.

**Acceptance Criteria:**

- [ ] Pressing `s` during `Running` state sends SIGTERM to the claude subprocess (via `Child::kill`) and transitions `AppState` to `Idle`
- [ ] When the claude subprocess exits for any reason, the runner task sends `RunnerEvent::Exited` to the main task
- [ ] On `RunnerEvent::Exited`, main task reloads the `Plan` from disk (ralph may have updated `passes: true`) and re-renders the Stories panel
- [ ] If ralph output contains the string `<promise>COMPLETE</promise>`, runner task sends `RunnerEvent::Complete`; main task transitions to `AppState::Complete` and reloads plan
- [ ] Max iterations is a constant (10) with a `// TODO: make configurable` comment; loop stops at max iterations
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-013: Interactive loop — pause between stories with continue prompt

**Description:** As a user, I want the loop to pause after each completed story and ask whether to continue so I can inspect results before the next iteration starts.

**Acceptance Criteria:**

- [ ] After `RunnerEvent::Exited` (plan not complete, loop not manually stopped), show a centered overlay: `Story done. Continue? [Y/n]  Next: <id>: <title>`
- [ ] Pressing `Y` or Enter closes overlay and starts the next ralph iteration (increments `AppState::Running.iteration`, spawns new claude subprocess)
- [ ] Pressing `n` or Esc closes overlay and transitions to `AppState::Idle` without starting next iteration
- [ ] If plan is complete after reload, no overlay is shown — transition directly to `AppState::Complete`
- [ ] Overlay is not shown if the loop was stopped via `[s]`
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-014: Migration helper — detect and migrate old scripts/ralph/ layout

**Description:** As a user with an existing project using `scripts/ralph/prd.json`, I want ralph-cli to detect the old layout on startup and offer to migrate it to `.ralph/plans/` with one keypress.

**Acceptance Criteria:**

- [ ] On startup, after `Store::find` succeeds: if `.ralph/` does not exist AND `scripts/ralph/prd.json` exists, show a migration overlay before opening normal TUI: `Old ralph layout detected in scripts/ralph/. Migrate to .ralph/plans/? [Y/n]`
- [ ] Pressing `Y`: reads `scripts/ralph/prd.json`, extracts `branchName`, strips `ralph/` or `feature/` prefix to use as plan name, creates `.ralph/plans/<name>/`, writes `prd.json` there, copies `scripts/ralph/progress.txt` if it exists; shows `Migrated to .ralph/plans/<name>/. Old files left in place.` and opens normal TUI
- [ ] Pressing `n`: skips migration and opens normal TUI
- [ ] If `branchName` after stripping is not a valid plan name, sanitize it: replace invalid characters with hyphens, truncate to 64 chars
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-015: Help overlay and top-level error handling

**Description:** As a user, I want a help overlay and graceful error handling so the tool is usable without reading documentation and never leaves the terminal in a broken state.

**Acceptance Criteria:**

- [ ] Pressing `?` opens a centered help overlay listing all keybindings with one-line descriptions; any key closes it
- [ ] Help overlay content: `j/k/↑↓ — navigate plans`, `r — run ralph loop`, `s — stop loop`, `n — new plan`, `e — edit prd.json`, `d — delete plan`, `? — help`, `q — quit`
- [ ] All `anyhow::Error` values reaching the top-level event loop are displayed in the status bar (truncated to fit) rather than panicking
- [ ] The panic hook set in US-004 is verified: terminal is restored before the panic message is printed
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

## Functional Requirements

- FR-1: Discover git root by walking up from current directory; reject non-git directories before opening TUI
- FR-2: All state stored under `<git-root>/.ralph/plans/<name>/prd.json`; schema is identical to current ralph format
- FR-3: Plans panel and Stories panel are always in sync; selecting a plan immediately updates stories view
- FR-4: Claude subprocess is spawned with `RALPH_PLAN_DIR` env var pointing to the active plan directory
- FR-5: Log output from claude is buffered (max 1000 lines) and rendered in the Log panel with auto-scroll
- FR-6: Loop pauses after each story exit with a `[Y/n]` continue overlay; does not auto-advance
- FR-7: `AppState` drives the status bar content and which keybindings are active
- FR-8: Terminal state is always restored on exit — normal quit, error exit, and panic

## Non-Goals

- No form-based PRD story editor inside the TUI (covered by a separate PRD)
- No PRD synthesis triggering from the TUI (covered by a separate PRD)
- No token usage tracking (covered by a separate PRD)
- No multi-repo support; one instance per repository
- No remote/cloud storage; everything is local files

## Technical Considerations

- Use `ratatui` + `crossterm` backend (not termion); crossterm is cross-platform
- Use `tokio` for async subprocess I/O; the event loop must remain responsive while claude is running
- `mpsc::unbounded_channel` carries both subprocess lines and keyboard events to the main loop
- `prd.json` schema must remain identical to what ralph agent reads/writes — do not change field names
- `cargo clippy -- -D warnings` must pass at all times; treat warnings as errors

## Success Metrics

- `ralph-cli` can fully replace `ralph.sh` and `ralph-status.sh` for day-to-day use
- User can create, run, and inspect a complete ralph loop without leaving the TUI
- Existing projects with `scripts/ralph/prd.json` can migrate with one keypress

## Open Questions

- Should `.ralph/` be added to `.gitignore` automatically on first run?
- Should the `RALPH_PLAN_DIR` env var be read by the ralph agent, or should the agent always use `prd.json` in the current directory?
