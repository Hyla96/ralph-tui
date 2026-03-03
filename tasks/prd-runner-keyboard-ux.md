# PRD: Runner Tab Keyboard UX Improvements

## Introduction

The runner tab currently intercepts many single-character keys (`s`, `a`, `q`, `k`, `j`, etc.) to provide app-level controls. This makes it impossible to type these characters into the Claude session running inside the PTY. This PRD introduces a Vim-style **Normal/Insert mode** for the runner tab, replaces the awkward `t + arrow` chord navigation with `Tab`/`Shift+Tab`, adds `[?]help` to the status bar, and displays a numeric index `[N]` next to each tab name so users can orient themselves visually.

## Goals

- Allow users to type freely into the Claude PTY session by pressing `i` to enter Insert mode
- Exit Insert mode back to app control with `Esc`
- Replace `t + Left/Right` tab navigation with `Tab` / `Shift+Tab`
- Display `[?]` in the runner status bar so users can see the help shortcut
- Show a `[N]` index prefix on every tab label in the tab bar for visual orientation

## User Stories

---

### US-001: Add Normal/Insert mode state to RunnerTab

**Description:** As a developer, I need a `insert_mode` boolean on `RunnerTab` so the key handler can branch between forwarding input to PTY vs. handling app shortcuts.

**Acceptance Criteria:**

- [ ] `RunnerTab` in `src/app.rs` gains a `pub insert_mode: bool` field (default `false`)
- [ ] Field is initialized to `false` in all `RunnerTab` construction sites (the two `RunnerTab { ... }` blocks in `start_runner` / reuse path)
- [ ] Field is reset to `false` when a tab is reused (the `tab.insert_mode = false` assignment in the reuse path of `start_runner`)
- [ ] `just check` passes

---

### US-002: Implement Insert mode key routing in the runner tab handler

**Description:** As a user, I want pressing `i` in Normal mode to enter Insert mode, and `Esc` to return to Normal mode, so I can type freely into the Claude session without the app intercepting letters.

**Acceptance Criteria:**

- [ ] In the runner tab `else` branch in `handle_events` (`src/app.rs` ~line 589), key routing checks `tab.insert_mode` first:
  - **Insert mode** (`insert_mode == true`):
    - `Esc` → set `tab.insert_mode = false`, do NOT forward Esc to PTY
    - `Ctrl+C` → forward `\x03` to PTY (interrupt signal); do NOT quit the app
    - All other keys → forward to PTY via `key_to_pty_bytes` (same path as the current `_ =>` fallthrough)
  - **Normal mode** (`insert_mode == false`):
    - `i` → set `tab.insert_mode = true` (do NOT forward `i` to PTY)
    - `Ctrl+C` → quit the app (existing behavior)
    - All other existing shortcuts (`a`, `x`, `q`, `k`, `j`, `G`, `?`) remain unchanged
    - `Tab` / `Shift+Tab` → cycle tabs (see US-003)
    - `Ctrl+S` (stop) replaces the old `s` shortcut (see US-004)
- [ ] The active `RunnerTab` borrow for reading `insert_mode` does not conflict with the mutable borrow needed to set it; use `let tab_idx = self.active_tab - 1` and index directly
- [ ] `just check` passes

---

### US-003: Replace `t + arrow` chord with `Tab` / `Shift+Tab` for tab navigation

**Description:** As a user, I want to navigate between tabs using `Tab` (next) and `Shift+Tab` (previous) in addition to the existing `t + digit` shortcut, and I want the `t + Left/Right` arrow chord removed since it conflicts with normal arrow-key use.

**Acceptance Criteria:**

- [ ] `KeyCode::Left` and `KeyCode::Right` cases are removed from `handle_tab_nav_key` in `src/app.rs` — only digit handling (`1`–`9`) remains
- [ ] `tab_nav_pending` flag and `handle_tab_nav_key` are **kept** (still needed for `t + digit` navigation)
- [ ] In both the **Workflows tab** handler and the **Normal mode** runner tab handler, these new bindings are added alongside the existing `t` chord:
  - `KeyCode::Tab` (no modifiers) → cycle to next tab (wrapping: last tab → 0)
  - `KeyCode::Tab` with `KeyModifiers::SHIFT` → cycle to previous tab (wrapping: 0 → last tab)
- [ ] In **Insert mode**, `Tab` is forwarded as `\t` to the PTY (not intercepted)
- [ ] `draw_runner_help_dialog` in `src/ui.rs` is updated: keep `t+1..9    switch tab` line, add `Tab         next tab` and `Shift+Tab   prev tab` lines
- [ ] `just check` passes

---

### US-004: Move `s` (stop) to `Ctrl+S` in Normal mode

**Description:** As a user, I want the stop command on the runner tab to use `Ctrl+S` rather than bare `s`, so that typing `s` in Insert mode works, and Normal mode shortcuts are less likely to conflict with future typing.

> **Note:** This is a consequence of the Insert/Normal mode split — in Normal mode the single-char shortcuts are fine because the user is not typing, but `s` is particularly dangerous (stops the runner accidentally). Moving it to `Ctrl+S` adds safety.

**Acceptance Criteria:**

- [ ] `KeyCode::Char('s') => self.stop_runner()` in the runner tab Normal mode handler is replaced with `KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => self.stop_runner()`
- [ ] The old `KeyCode::Char('s')` match arm is removed
- [ ] Runner status bar hint updated: `[s]top` → `[Ctrl+S]stop` (in `draw_runner_tab` in `src/ui.rs`)
- [ ] `draw_runner_help_dialog` updated: `s  stop loop` → `Ctrl+S  stop loop`
- [ ] `just check` passes

---

### US-005: Add mode indicator and `[?]help` to the runner tab status bar

**Description:** As a user, I want to see the current mode (INSERT/NORMAL) and a `[?]help` hint in the runner tab status bar, so I know what mode I'm in and how to access help.

**Acceptance Criteria:**

- [ ] `draw_runner_tab` in `src/ui.rs` reads `tab.insert_mode` and renders the mode indicator:
  - **Insert mode**: left side of status bar shows `-- INSERT --  [Esc] normal mode`, styled in a distinct color (e.g. `Color::Green`)
  - **Normal mode / Running**: existing `[Ctrl+S]stop  [a]uto:ON/OFF  [?]help` (adding `[?]help`)
  - **Normal mode / Done**: `[x]close  [?]help`
  - **Normal mode / Error**: `[x]close  [q]quit  [?]help` (styled red as before)
- [ ] The right-aligned task context (iteration, cost, etc.) is still shown when not in Insert mode
- [ ] `just check` passes

---

### US-006: Reset Insert mode when a runner tab reaches Done or Error state

**Description:** As a user, I want Insert mode to automatically exit when the runner finishes (Done or Error), so I'm not left in a mode where my keystrokes silently go to a dead PTY.

**Acceptance Criteria:**

- [ ] Every place in `src/app.rs` where `tab.state` is set to `RunnerTabState::Done` or `RunnerTabState::Error(...)`, also set `tab.insert_mode = false`
  - Locations: `drain_runner_channels` done/error transitions, `stop_runner` path, and the `spawn_next_iteration` auto-loop completion path
- [ ] A tab that was in Insert mode while running transitions cleanly to Normal mode status bar display when it finishes
- [ ] `just check` passes

---

### US-007: Show `[N]` index on each tab label in the tab bar

**Description:** As a user, I want each tab to show a numeric index prefix (e.g. `[1] Workflows`, `[2] my-workflow`) in the tab bar so I can visually orient myself among multiple open tabs.

**Acceptance Criteria:**

- [ ] `draw_tab_bar` in `src/ui.rs` prefixes each tab label with its 1-based index: Workflows → `[1] Workflows`, first runner tab → `[2] <workflow_name>`, second runner → `[3] <workflow_name>`, etc.
- [ ] The index is part of the label string passed to the `entries` vec, so existing styling (REVERSED+BOLD for active, DIM for inactive, CLAUDE_ORANGE for runner tabs) applies to the whole label including the index
- [ ] Status suffix (`✓`, `!`) still appears after the workflow name: e.g. `[2] my-workflow ✓`
- [ ] The tab bar width overflow truncation logic (`used_width > area.width` check) still works correctly with the wider labels
- [ ] `just check` passes

---

### US-008: Update `draw_runner_help_dialog` to reflect new shortcuts

**Description:** As a user, I want the `?` help dialog to show the current, accurate list of runner tab shortcuts.

**Acceptance Criteria:**

- [ ] Dialog in `src/ui.rs` `draw_runner_help_dialog` shows (Normal mode section, then Insert mode section):
  - `--- Normal mode ---`
  - `i           enter insert mode`
  - `Ctrl+S      stop loop`
  - `a           toggle auto-continue`
  - `↑/k         scroll up`
  - `↓/j         scroll down`
  - `End/G       jump to bottom`
  - `Tab         next tab`
  - `Shift+Tab   prev tab`
  - `x           close tab`
  - `?           this help`
  - `q           quit`
  - `--- Insert mode ---`
  - `Esc         back to normal mode`
  - `Ctrl+C      send interrupt to PTY`
- [ ] Dialog width is wide enough to fit `Shift+Tab   prev tab` (currently 46 wide; increase to 50)
- [ ] Dialog height is adjusted to fit all lines (currently 11 tall; 2 border + 15 content rows = 17 tall)
- [ ] `just check` passes

---

## Functional Requirements

- FR-1: `RunnerTab` has an `insert_mode: bool` field defaulting to `false`
- FR-2: In runner tab Normal mode, `i` enters Insert mode; in Insert mode, `Esc` exits back to Normal mode
- FR-3: In Insert mode, all keys (except `Esc`) are forwarded as raw bytes to the PTY
- FR-4: In Insert mode, `Ctrl+C` forwards `\x03` to the PTY instead of quitting the app
- FR-5: `Tab` (next) and `Shift+Tab` (previous) navigate between tabs in Normal mode and the Workflows tab
- FR-5b: `t + 1..9` digit chord navigation is preserved unchanged
- FR-5c: `t + Left/Right` arrow chord is removed
- FR-6: `Tab` in Insert mode is forwarded to PTY as `\t`
- FR-7: Stop shortcut is `Ctrl+S` (not bare `s`) in Normal mode on runner tabs
- FR-8: Runner status bar displays `-- INSERT --` with `[Esc] normal mode` hint while in Insert mode
- FR-9: Runner status bar shows `[?]help` hint in all Normal mode states
- FR-10: Help dialog content reflects all current shortcuts accurately
- FR-11: `insert_mode` resets to `false` whenever a tab transitions to Done or Error state
- FR-12: Each tab in the tab bar is prefixed with its 1-based index: `[1] Workflows`, `[2] <name>`, etc.

## Non-Goals

- No per-tab mode persistence across tab switches (mode resets when returning to a tab is acceptable)
- No visual cursor in the status bar for Insert mode beyond the text indicator
- No Insert mode for the Workflows tab
- `t + digit` shortcuts are preserved; only `t + Left/Right` is removed
- Tab index labels are visual only and correspond to the `t + digit` mnemonic (`[1]` = `t+1`, etc.)

## Technical Considerations

- `tab_nav_pending` and `handle_tab_nav_key` are kept; only the `KeyCode::Left` / `KeyCode::Right` arms inside `handle_tab_nav_key` are deleted
- `KeyCode::Tab` with `KeyModifiers::SHIFT` is how crossterm reports `Shift+Tab`
- In Insert mode, `key_to_pty_bytes` already handles `KeyCode::Tab → b'\t'`, so no special-casing needed
- Borrowing: read `insert_mode` by indexing `self.runner_tabs[tab_idx].insert_mode` (copy of bool), then mutate after. Or use `let insert_mode = tab.insert_mode;` before any `&mut self` call.
- The `draw_runner_tab` function takes `&App` (immutable), so `tab.insert_mode` is directly readable
- `Ctrl+C` in Insert mode: match `KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL)` before the generic key-forward fallthrough, and send `vec![0x03]` directly to `stdin_tx` (bypassing `key_to_pty_bytes` which may not handle this)
- Tab index labels: in `draw_tab_bar`, the `entries` vec is built with `(format!(" [{}] {} ", idx+1, label), ...)` — the `idx` is the loop index (0-based), so `idx+1` gives the 1-based display number. The runner tab status suffix (`✓`, `!`) is appended inside the format string as before.

## Success Metrics

- User can type `s`, `a`, `q`, `k`, `j`, `t` freely into a Claude session in Insert mode
- No accidental runner stops when typing in the PTY
- Tab navigation works with standard terminal `Tab`/`Shift+Tab` muscle memory
- `[?]` is visible in the status bar at all times in Normal mode
- Tab indices are clearly visible so users can orient among multiple open runners
