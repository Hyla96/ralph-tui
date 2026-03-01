# PRD: Tabbed Runner UI with Claude Code Interaction

## Introduction

Replace the single-pane UI with a tab-based layout. The first tab ("Workflows") contains the
existing workflow list and task panel. Each time a workflow runner is started, a new runner tab
is auto-created (named after the workflow) that shows live log output from that `claude` process.
Multiple runners can operate in parallel, each in its own tab. When Claude Code asks a question or
presents options, the user can type directly into an input field at the bottom of the active runner
tab and press Enter to send the response.

## Goals

- Show a persistent tab bar across the top of the TUI.
- Keep the "Workflows" management panel in tab 0.
- Auto-create one runner tab per running workflow; allow multiple simultaneous runners.
- Show live stdout/stderr log output inside each runner tab.
- Pipe stdin to the claude subprocess so the user can answer Claude Code prompts.
- Mark runner tabs as "Done" (visually) when the subprocess exits; let the user close them manually.
- Navigate tabs with `t` + digit or `t` + left/right arrow.

## User Stories

### US-001: Runner tab data model

**Description:** As a developer, I need a data structure to hold per-runner state so that multiple
runners can coexist without sharing mutable fields.

**Acceptance Criteria:**

- [ ] Add a `RunnerTab` struct in `app.rs` with fields:
  - `workflow_name: String`
  - `log_lines: Vec<String>` (capped at 1 000 lines, oldest dropped)
  - `state: RunnerTabState` (enum: `Running { iteration: u32 }`, `Done`, `Error(String)`)
  - `runner_rx: Option<UnboundedReceiver<RunnerEvent>>`
  - `runner_kill_tx: Option<oneshot::Sender<()>>`
  - `stdin_tx: Option<UnboundedSender<String>>` (for sending user input to the process)
  - `input_buffer: String` (text the user is typing in the input field)
  - `log_scroll: usize` (scroll offset for the log view)
- [ ] Remove the flat `log_lines`, `runner_rx`, `runner_kill_tx`, and `app_state` fields from
  `App`. Keep `AppState` enum but use it only for the Workflows tab's idle/running indicator if
  needed; runner-specific state now lives inside `RunnerTab`.
- [ ] Add `runner_tabs: Vec<RunnerTab>` and `active_tab: usize` (0 = Workflows tab) to `App`.
- [ ] `cargo build` and `cargo clippy -- -D warnings` pass.

### US-002: Tab bar rendering

**Description:** As a user, I want to see all open tabs at the top of the screen so I know what is
available and which tab is active.

**Acceptance Criteria:**

- [ ] A single-line tab bar is drawn at the very top of the terminal.
- [ ] Tab 0 is always labelled `[Workflows]`.
- [ ] Each runner tab is labelled `[<workflow-name>]` while running and `[<workflow-name> ✓]` when
  Done (or `[<workflow-name> !]` on error).
- [ ] The active tab is highlighted (reversed/bold style).
- [ ] If tabs overflow the width of the terminal, visible tabs are cropped (no scrolling required
  in v1).
- [ ] The rest of the screen below the tab bar renders the content of the active tab.
- [ ] `cargo build` and `cargo clippy -- -D warnings` pass.
- [ ] Verify visually in the running TUI.

### US-003: Tab navigation (t-prefix chords)

**Description:** As a user, I want to switch tabs with `t` + a digit or `t` + an arrow key so I
can jump to any tab without leaving the keyboard home row.

**Acceptance Criteria:**

- [ ] Pressing `t` sets a boolean flag `tab_nav_pending: bool` on `App` and waits for the next
  keypress.
- [ ] While `tab_nav_pending` is true:
  - Pressing digit `1`–`9` switches to tab at index digit − 1 (so `1` = Workflows, `2` = first
    runner tab, etc.). If the index is out of range, the keypress is ignored.
  - Pressing `Left` or `Right` arrow cycles to the previous / next tab (wrapping).
  - Any other key clears `tab_nav_pending` with no tab change.
- [ ] `tab_nav_pending` is cleared after any resolution (successful or not).
- [ ] When active tab changes, the previous tab's input focus and state are preserved.
- [ ] `cargo build` and `cargo clippy -- -D warnings` pass.

### US-004: Workflows tab content (no regression)

**Description:** As a user, I want the Workflows tab to look and behave exactly as the current UI
does so that existing workflow management is not disrupted.

**Acceptance Criteria:**

- [ ] When `active_tab == 0`, the full Workflows layout (workflow list left, tasks right, log
  panel, status bar) is rendered exactly as before this feature.
- [ ] All existing keybindings (`n`, `d`, `r`, `s`, `e`, `?`, `q`, arrows) work normally while the
  Workflows tab is active.
- [ ] Pressing `r` on a selected workflow while the Workflows tab is active starts the runner AND
  switches focus to the newly created runner tab.
- [ ] `cargo build` and `cargo clippy -- -D warnings` pass.
- [ ] Verify visually in the running TUI.

### US-005: Runner tab log view

**Description:** As a user, I want to see the live stdout/stderr of the running claude process in
the runner tab so I can follow its progress.

**Acceptance Criteria:**

- [ ] When the active tab is a runner tab, the terminal area below the tab bar is split vertically:
  log view (fills available height minus 3 lines for the input row) on top, input row on the
  bottom.
- [ ] Log lines are rendered newest-at-bottom (append style).
- [ ] The log view auto-scrolls to the bottom as new lines arrive unless the user has manually
  scrolled up.
- [ ] The user can scroll the log with `Up`/`Down` (or `k`/`j`) while in the runner tab; scrolling
  up disables auto-scroll until `End` or `G` is pressed to jump back to the bottom and re-enable
  auto-scroll.
- [ ] The runner tab shows a status line (e.g., `Running — iteration 2/10` or `Done`) above the
  input box.
- [ ] `cargo build` and `cargo clippy -- -D warnings` pass.
- [ ] Verify visually in the running TUI.

### US-006: Stdin pipe to the claude subprocess

**Description:** As a developer, I need the claude subprocess to have a writable stdin pipe so the
user's typed responses can be delivered to it.

**Acceptance Criteria:**

- [ ] `runner_task` signature gains a new parameter: `stdin_rx: UnboundedReceiver<String>`.
- [ ] Inside `runner_task`, `Command::stdin(Stdio::piped())` is used; `child.stdin.take()` gives a
  writable handle.
- [ ] A dedicated Tokio task inside `runner_task` reads lines from `stdin_rx` and writes them
  (appending `\n`) to the child's stdin handle using `tokio::io::AsyncWriteExt`.
- [ ] If the stdin channel closes or the write fails, the stdin task exits silently (the child
  process may have already exited).
- [ ] The corresponding `UnboundedSender<String>` is stored in `RunnerTab::stdin_tx`.
- [ ] `cargo build` and `cargo clippy -- -D warnings` pass.

### US-007: Input field and send-to-claude

**Description:** As a user, I want a text input field at the bottom of the runner tab where I can
type a response and press Enter to send it to Claude Code so I can answer its questions or select
options.

**Acceptance Criteria:**

- [ ] The bottom row of every active runner tab shows a labelled input field: `> ` followed by the
  current `input_buffer` content and a blinking cursor block (ratatui `Paragraph` with the buffer
  as text suffices; a real cursor offset is not required for v1).
- [ ] While the active tab is a runner tab (and no dialog is open):
  - Printable characters are appended to `RunnerTab::input_buffer`.
  - `Backspace` removes the last character.
  - `Enter` sends the contents of `input_buffer` (trimmed) via `stdin_tx`, then clears
    `input_buffer`.
  - `Esc` clears `input_buffer` without sending.
- [ ] Scrolling keys (`Up`/`Down`, `k`/`j`) still scroll the log; they do not affect the input
  buffer.
- [ ] If `stdin_tx` is `None` (runner is done), pressing Enter shows a transient status message
  "Runner is not active" for 2 s.
- [ ] `cargo build` and `cargo clippy -- -D warnings` pass.
- [ ] Verify visually: type a message and confirm it appears sent in a test run.

### US-008: Runner tab lifecycle (create, done, close)

**Description:** As a user, I want runner tabs to be created automatically and to persist with a
"Done" indicator when the runner exits so I can review output after completion.

**Acceptance Criteria:**

- [ ] When `start_runner` is called for workflow `<name>`:
  - If a runner tab for `<name>` already exists and is in `Done`/`Error` state, it is reused
    (log cleared, state reset to `Running { iteration: 1 }`).
  - Otherwise a new `RunnerTab` is appended to `runner_tabs`.
  - `active_tab` is set to the index of this tab so focus jumps there immediately.
- [ ] When a runner exits (via `RunnerEvent::Exited`):
  - `runner_rx`, `runner_kill_tx`, and `stdin_tx` are set to `None`.
  - `RunnerTab::state` transitions to `Done` (or `Error` on `SpawnError`).
  - The tab label gains the ` ✓` / ` !` suffix.
  - A final log line `"--- Runner exited ---"` is appended.
  - The user is NOT automatically redirected to the Workflows tab; they stay on the runner tab.
- [ ] The user can close a Done/Error runner tab by pressing `x` while that tab is active.
  - `active_tab` moves to the previous tab (or 0 if none).
- [ ] Running tabs cannot be closed with `x`; a transient status message "Stop the runner first
  [s]" is shown for 2 s.
- [ ] The ContinuePrompt dialog still works per-runner-tab (shown inside the runner tab, not as a
  global dialog).
- [ ] `cargo build` and `cargo clippy -- -D warnings` pass.
- [ ] Verify visually in the running TUI.

## Functional Requirements

- FR-1: A tab bar is always visible at the top of the TUI; tab 0 is always the Workflows tab.
- FR-2: One runner tab is created per workflow run (or reused if the prior run is done); multiple
  runner tabs can be active simultaneously.
- FR-3: Tab navigation uses a `t`-prefix chord: `t` then digit or `t` then arrow.
- FR-4: Each runner tab displays live log output with scroll support and auto-scroll-to-bottom.
- FR-5: Each runner tab has an always-visible text input field; Enter sends the text to the claude
  process via a piped stdin channel.
- FR-6: Runner tabs persist after the subprocess exits, labelled with ` ✓` (done) or ` !` (error).
- FR-7: Done/Error runner tabs can be closed with `x`; running tabs cannot.
- FR-8: Pressing `r` on the Workflows tab automatically switches focus to the new/reused runner tab.

## Non-Goals

- No PTY / pseudo-terminal emulation (ANSI escape codes from claude will be stored as-is and
  rendered as raw text; colour stripping can be a follow-up).
- No tab reordering or drag-and-drop.
- No tab scrolling when labels overflow the terminal width (truncation only in v1).
- No per-tab iteration cap configuration (still uses the global `MAX_ITERATIONS`).
- No persistence of runner tabs across application restarts.

## Technical Considerations

- The current single `runner_rx / runner_kill_tx` fields on `App` are replaced by per-tab fields
  inside `RunnerTab`. `drain_runner_channel` must be updated to iterate over all tabs.
- `runner_task` gains a `stdin_rx: UnboundedReceiver<String>` parameter and pipes stdin.
  Callers must create an `UnboundedChannel` and pass both ends appropriately.
- `AppState` on `App` may be removed entirely if all runner state moves into `RunnerTab`; the
  Workflows tab itself has no Running/Idle concept after this refactor.
- Tab bar rendering lives in `ui.rs`; consider a helper `draw_tab_bar(frame, app, area)`.
- `tab_nav_pending` is a field on `App` reset on every event resolution; no timeout is required
  (first non-`t` key after `t` resolves the chord).
- Log auto-scroll: track `RunnerTab::auto_scroll: bool`; set to `false` when user scrolls up,
  reset to `true` when user presses `End`/`G` or when the tab is newly created.

## Success Metrics

- Multiple workflows can be run simultaneously with independent output visible in separate tabs.
- User can answer a Claude Code interactive prompt by typing in the input field and pressing Enter.
- No regression in the Workflows tab — existing keybindings and workflow CRUD all work correctly.

## Open Questions

- Should ANSI escape codes in claude output be stripped (for readability) or preserved? (Assumed
  stripped / rendered as raw for v1; a follow-up can add a proper ANSI parser.)
- Should the ContinuePrompt dialog appear as an overlay inside the runner tab, or should it be
  replaced by a line in the log + a prompt in the input field? (The overlay approach is simpler
  and assumed for v1.)
