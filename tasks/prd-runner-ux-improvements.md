# PRD: Runner UX Improvements

## Introduction

This PRD covers three related improvements to the runner tab UI and behavior:

1. **Bug fix**: The auto-run loop (`auto_continue=true`) doesn't restart after task completion — it silently does nothing.
2. **[c]ontinue button**: A new manual-continue button that replaces the modal `ContinuePrompt` dialog, grayed out when auto-run is enabled.
3. **Layout refactor**: Move the task title to its own row below the tab bar; keep only action buttons on the bottom status bar.

## Goals

- Auto-run reliably loops through tasks without user intervention.
- Manual continuation feels natural (button in status bar, not a blocking dialog).
- The runner tab header is uncluttered: task context on top, controls on bottom.

## User Stories

---

### US-001: Fix the auto-run loop bug

**Description:** As a user, I want auto-run to automatically spawn the next iteration when a task completes so that the loop keeps running without me having to intervene.

**Context / suspected root cause:**

The logic lives in `src/app.rs` → `drain_tab_channel`. When a `RunnerEvent::Complete` arrives (the `<promise>COMPLETE</promise>` sentinel):

- If `complete=true && done=false && is_auto=true`: `spawn_next_iteration_at` is called immediately. **Problem**: at this point the old `runner_kill_tx` is **dropped** by `spawn_next_iteration_at` (it replaces it with a new one). Dropping the `oneshot::Sender` causes the `kill_rx` in the old `runner_task`'s `tokio::select!` to fire with `Err(RecvError)`, which matches `_ = kill_rx` and sets `was_killed=true` — causing `killer.kill()` to be called on the still-running old process. This premature kill might happen before the old process updates `prd.json` (marks the task as `passes: true`). Then when we load the workflow to check `is_complete`, the task is not marked done, so the loop keeps respawning the same task.

- If `complete=true && done=true && is_auto=true`: falls through to the `done` block, which checks `iteration_opt`. This path appears correct but should be verified.

The fix should ensure:
1. The old process is not killed prematurely when it's still updating `prd.json` after emitting the complete sentinel.
2. The `done` block correctly spawns the next iteration when `auto_continue=true`.
3. A minimum wait or sequencing that ensures `prd.json` is up to date before deciding `is_complete`.

**Implementation notes:**

- Primary fix: in the `complete && !done && is_auto` branch, instead of calling `spawn_next_iteration_at` immediately, wait for the `Exited` event (i.e., let the `done` block handle everything). This avoids premature kill. The simplest way: **remove the `complete && !done && is_auto` early-spawn branch entirely** and handle everything in the `done` block.
  - When `complete && !done`: just set a flag. Don't spawn yet.
  - When `done` (regardless of whether `complete` was true earlier or in this tick): run the full auto-loop decision with access to `complete` (pass it as state or local variable — it's already a local).
- With this change, the old process runs to completion (writes `prd.json`), exits, and only then do we spawn the next iteration.
- The `complete` local variable already covers both cases (true if received in this tick or in a previous tick via `complete && !done` path, but since we're removing that path, it's just whether Complete and Exited both arrived in this tick).

Actually: since Complete always arrives before Exited in the FIFO channel, when `done=true`, `complete` captures whether Complete arrived in the same drain tick. But if Complete arrived in a PREVIOUS tick (where `done=false`), the current tick would have `complete=false` when Exited arrives — the `done` block would use stale `complete=false`. To fix this, add a `saw_complete` boolean field to `RunnerTab` that persists across ticks.

**Plan:**
1. Add `pub saw_complete: bool` to `RunnerTab` (initialized to `false`, reset to `false` on new iteration).
2. In `drain_tab_channel`: when `RunnerEvent::Complete` is received, set `self.runner_tabs[tab_idx].saw_complete = true` (in addition to local `complete = true`).
3. Remove the `complete && !done && is_auto` early-spawn block entirely.
4. In the `done` block, use `self.runner_tabs[tab_idx].saw_complete` in place of `complete` for `is_success` calculation, and reset it afterwards.
5. The old process now runs to natural completion, `prd.json` is up to date by the time `Exited` fires.

**Acceptance Criteria:**

- [ ] `RunnerTab` has a `saw_complete: bool` field, initialized `false`, reset on each new iteration start (in `start_runner` and `spawn_next_iteration_at`).
- [ ] When `RunnerEvent::Complete` is received in `drain_tab_channel`, `tab.saw_complete` is set `true`.
- [ ] The `complete && !done && is_auto` early-spawn block is removed.
- [ ] In the `done` block, `is_success` is `tab.saw_complete || exit_code==0`; `tab.saw_complete` is reset to `false` after the iteration ends.
- [ ] `auto_continue=true` mode reliably loops through multiple tasks without user input.
- [ ] `auto_continue=false` mode still shows the `ContinuePrompt` dialog (unchanged).
- [ ] `just check` passes.

---

### US-002: Add [c]ontinue button; remove ContinuePrompt dialog

**Description:** As a user, I want to continue to the next task by pressing `[c]` in the runner tab status bar instead of responding to a blocking dialog, so that the interaction feels more integrated with the TUI.

**Behavior:**

- When `auto_continue=false` and state is `RunnerTabState::Done` (task completed, workflow not fully done): the buttons bar shows a `[c]ontinue` button. Pressing `c` spawns the next iteration (same as confirming the old `ContinuePrompt`).
- When `auto_continue=true`: the `[c]` label is shown but styled dimmed/gray to indicate it's inactive (auto handles it).
- The `Dialog::ContinuePrompt` variant and its dialog rendering can be removed entirely (or kept but no longer shown).
- Key `c` in Normal mode on a runner tab with state Done and `auto_continue=false`: calls `spawn_next_iteration` (same logic as `handle_dialog_key` for `ContinuePrompt` confirmation).
- Key `c` on a Running tab or when `auto_continue=true`: no-op (or ignored).

**Acceptance Criteria:**

- [ ] `Dialog::ContinuePrompt` is no longer shown after task completion (remove the code that sets `self.dialog = Some(Dialog::ContinuePrompt {...})` in the `done` block).
- [ ] The `Dialog::ContinuePrompt` variant, its `draw_continue_prompt_dialog` render function, and its `handle_dialog_key` branch can be removed (they are dead code once the above is done).
- [ ] `[c]ontinue` label appears in the buttons bar when state is `Done` and `auto_continue=false`.
- [ ] `[c]` is shown dimmed/gray when state is `Done` and `auto_continue=true`.
- [ ] Pressing `c` in Normal mode when state is `Done` and `auto_continue=false` calls `spawn_next_iteration_at` for the active tab.
- [ ] The Runner tab keybindings (`handle_events` Normal mode branch) handles `KeyCode::Char('c')` appropriately: continue if Done+manual, no-op otherwise.
- [ ] `just check` passes.

---

### US-003: Separate task title bar from buttons bar

**Description:** As a user, I want the current task title (and context: tasks done/total, iteration, tokens) shown on its own row at the top of the runner tab area, below the tab bar, so that the bottom status bar contains only action buttons and is easy to scan.

**Current layout (runner tab):**
```
┌─ tab bar ─────────────────────────────────────────────────────┐  ← 1 row
│                                                               │
│  PTY/log (block border with task title "{id}: {title}")       │  ← flexible
│                                                               │
└───────────────────────────────────────────────────────────────┘
 [i]nsert  [s]top  [a]uto:ON  [?]help   task-title  4/8  iter2  ← 1 row (status)
```

**New layout (runner tab):**
```
┌─ tab bar ─────────────────────────────────────────────────────┐  ← 1 row (unchanged)
 US-003: Implement login flow  4/8 tasks  iter 2  tok: 12,345    ← 1 row NEW (task bar)
┌───────────────────────────────────────────────────────────────┐
│                                                               │
│  PTY/log (block border title = "Runner: {workflow_name}")     │  ← flexible
│                                                               │
└───────────────────────────────────────────────────────────────┘
 [i]nsert  [s]top  [a]uto:ON  [c]ontinue  [?]help  [q]uit       ← 1 row (buttons)
```

**Implementation notes:**

- In `draw_runner_tab` (`src/ui.rs`), change the layout from 2 rows to 3 rows:
  ```rust
  let layout = Layout::default()
      .direction(Direction::Vertical)
      .constraints([
          Constraint::Length(1),  // task title bar (NEW)
          Constraint::Min(0),     // PTY viewport
          Constraint::Length(1),  // buttons bar
      ])
      .split(area);
  ```
- Render task context (currently built by `runner_tab_context`) as `layout[0]` (the new top bar).
- Render PTY in `layout[1]` (the viewport).
- Render action buttons in `layout[2]` (the bottom buttons bar).
- The PTY block border title changes to just the workflow name: `"Runner: {workflow_name}"` (or no title).
- The task title bar: left-align task title, right-align `{done}/{total} tasks  iter {n}  tok: {n}`.
- Increment `PTY_ROW_OVERHEAD` from `4` to `5` in `src/app.rs` (one extra row for the task bar).
- The `notification_right_spans` helper can be reused or replaced for the task bar's right-aligned content.
- For `Error` state: task bar can show the workflow name only (no task title).
- For `Insert` mode: the task bar remains visible (it's not the buttons bar).

**Acceptance Criteria:**

- [ ] Runner tab has 3 vertical sections: task bar (top), PTY viewport (middle), buttons bar (bottom).
- [ ] Task bar (top, 1 row) shows: current task title (truncated if needed), tasks done/total, iteration count, token usage — left to right with appropriate padding.
- [ ] Buttons bar (bottom, 1 row) shows only action buttons (no task context).
- [ ] PTY block border title shows `"Runner: {workflow_name}"` only (task title removed from border).
- [ ] `PTY_ROW_OVERHEAD` is updated to `5`.
- [ ] `[c]ontinue` button appears in the buttons bar (from US-002) in the correct states.
- [ ] Insert mode: task bar still visible; buttons bar shows `-- INSERT -- [Esc] normal mode`.
- [ ] Error state: task bar shows workflow name or nothing; buttons bar shows error-colored hints.
- [ ] `just check` passes.

---

## Functional Requirements

- FR-1: Auto-run loop uses `saw_complete` flag persisted across drain ticks; early-spawn-on-Complete is removed.
- FR-2: `[c]ontinue` button in the buttons bar replaces the `ContinuePrompt` modal dialog.
- FR-3: `[c]ontinue` is rendered dimmed when `auto_continue=true`; active when `auto_continue=false` and state is `Done`.
- FR-4: Task context (title, done/total, iter, tokens) is displayed on a dedicated row above the PTY viewport.
- FR-5: Bottom status bar contains only action keybindings.
- FR-6: `PTY_ROW_OVERHEAD` updated to account for the new task bar row.

## Non-Goals

- No change to auto_continue toggle behavior (still `[a]` key).
- No change to the workflow tab layout.
- No new configuration options (max iterations, timeouts, etc.).
- No change to the PRD editor.

## Technical Considerations

- `drain_tab_channel` is called every 100 ms (event poll); the `saw_complete` field persists between calls within the same runner lifecycle.
- Removing `ContinuePrompt` also removes the `Dialog::ContinuePrompt` variant; check `match app.dialog` arms in `ui.rs` and `handle_dialog_key` in `app.rs` are updated.
- `PTY_ROW_OVERHEAD` is used in both `start_runner` and `handle_events` (resize) — update all usages.

## Success Metrics

- Auto-run mode runs through all pending tasks without any user intervention.
- [c]ontinue replaces the blocking dialog, making the UX more consistent with TUI conventions.
- The runner tab top row clearly shows what task is running and progress at a glance.

## Open Questions

- Should the task bar always be visible, or only for runner tabs (not when workflows tab is active)? (Assumed: runner tabs only.)
- Should the PTY block retain its title/border at all, or go borderless to save space? (Assumed: keep border but use workflow name as title.)
