# PRD: ralph-cli File Watcher (Live Reload)

## Introduction

Add OS-native file watching to ralph-cli so the TUI automatically reloads plan data whenever any `.json` file in the repository changes. This removes the need to manually restart the TUI after `prd.json` is edited externally (e.g., by the ralph agent, by the user's editor, or by another tool). The implementation uses the `notify` crate's OS-native backends (FSEvents on macOS, inotify on Linux, ReadDirectoryChanges on Windows) so the idle CPU cost is zero.

## Goals

- Detect changes to any `.json` file in the repo root (recursive) using OS-native events
- Reload all in-memory state (plan list, current plan, runner state) immediately on change
- Debounce rapid successive events to avoid thrashing during atomic editor saves
- Show a brief status bar notification identifying the changed file

## User Stories

### US-001: Add `notify` dependency and watcher module

**Description:** As a developer, I need a `Watcher` struct that monitors the repo root for `.json` file changes using OS-native events and sends debounced events to the app so the rest of the system has a reliable, performant source of file change notifications.

**Acceptance Criteria:**

- [ ] `Cargo.toml` adds `notify` (v6) and `notify-debouncer-full` (compatible version with notify v6)
- [ ] `src/ralph/watcher.rs` is created and declared in `src/ralph/mod.rs`
- [ ] `WatcherEvent` struct is defined: `{ path: PathBuf }` (path of the changed `.json` file)
- [ ] `Watcher` struct holds the debouncer handle (to keep it alive for the duration of the watch)
- [ ] `Watcher::start(root: &Path, tx: tokio::sync::mpsc::Sender<WatcherEvent>) -> anyhow::Result<Watcher>` creates a `new_debouncer` with a 50 ms debounce timeout using the OS-native `RecommendedWatcher` backend, watches `root` recursively, and sends a `WatcherEvent` for each debounced event whose path has a `.json` extension; non-`.json` events are silently dropped
- [ ] Events of kind `Create`, `Modify`, and `Remove/Rename` are all forwarded; metadata-only events (e.g. access time) are dropped
- [ ] The returned `Watcher` keeps the debouncer alive (drop = stop watching)
- [ ] `just check` passes

---

### US-002: Integrate watcher into the app event loop

**Description:** As a developer, I need the app to start the watcher on launch and react to `WatcherEvent` messages in the main event loop so plan data is always current without user intervention.

**Acceptance Criteria:**

- [ ] `App` holds a `tokio::sync::mpsc::Receiver<WatcherEvent>` and a `Watcher` handle (to keep the OS watcher alive)
- [ ] During `App::new()` (or equivalent startup), `Watcher::start` is called with the repo root from `Store`; if the watcher fails to start, the app logs a warning to the Log panel and continues running without watching (graceful degradation)
- [ ] The main event loop drains the watcher channel each tick (non-blocking `try_recv` loop) before processing keyboard events, collecting all pending `WatcherEvent`s into a local `Vec` before acting on them (runner channel drain pattern from CLAUDE.md)
- [ ] When one or more `WatcherEvent`s are received in a tick, `app.reload_all()` is called exactly once — regardless of how many events arrived — then the TUI is re-rendered
- [ ] `reload_all()` re-runs `Store::list_plans()` to refresh the plan list, reloads the currently selected plan from disk via `Plan::load`, and resets any runner state that references stale plan data (e.g. clears the pending story if it no longer exists in the reloaded plan)
- [ ] If the currently selected plan no longer exists after reload (file deleted), selection falls back to the first available plan, or clears if the list is empty
- [ ] A reload triggered by the watcher does not interrupt an active runner subprocess — the runner continues, and the reloaded data reflects the new on-disk state (answer 3B: immediate, non-blocking refresh)
- [ ] `just check` passes

---

### US-003: Status bar notification on watcher-triggered reload

**Description:** As a user, I want to see a brief notification in the status bar when the TUI automatically reloads due to a file change so I know the display is current and what triggered the update.

**Acceptance Criteria:**

- [ ] `App` state gains a `notification: Option<(String, std::time::Instant)>` field holding the message text and the time it was set
- [ ] After each `reload_all()` triggered by a watcher event, `notification` is set to a message in the format: `↻ <relative_path> reloaded` where `<relative_path>` is the changed file's path relative to the repo root (e.g. `.ralph/plans/my-feature/prd.json`)
- [ ] If multiple `.json` files change in the same tick, the notification shows the first path from the collected events (the others still trigger a unified reload)
- [ ] The status bar renders the notification text when `notification` is `Some` and the elapsed time since the `Instant` is less than 3 seconds; after 3 seconds the field is set to `None` and the status bar reverts to its normal content
- [ ] The 3-second expiry is checked on every render tick (the existing 100 ms `event::poll` cadence is sufficient — no additional timer needed)
- [ ] Notification text is shown on the right side of the status bar, truncated with `…` if it would overflow the terminal width
- [ ] `just check` passes

---

## Functional Requirements

- FR-1: Use OS-native file watching backends via `notify`/`notify-debouncer-full` — no polling loops
- FR-2: Watch the entire repo root recursively; filter to `.json` extension only
- FR-3: Debounce interval is 50 ms to absorb atomic editor saves (write-to-temp + rename patterns)
- FR-4: A single `reload_all()` call per tick regardless of how many events arrived
- FR-5: Watcher startup failure is non-fatal — app continues without watching
- FR-6: Active runner subprocess is never interrupted by a reload
- FR-7: Status bar notification auto-clears after 3 seconds; no persistent notification state

## Non-Goals

- No watching of non-`.json` files (e.g. `.jsonl`, `.md`, source files)
- No per-file partial reload — always a full `reload_all()` for simplicity
- No notification sound, desktop notification, or log panel entry on reload
- No configurable debounce interval or watch path in this iteration
- No watcher on Windows (the `notify` crate supports it, but it is not a target platform for this project yet — the abstraction must not break on Windows, but is not tested)

## Technical Considerations

- `notify-debouncer-full` runs its internal callback on a background thread; bridge to the async tokio world using `tokio::sync::mpsc::channel` with a small buffer (e.g. 32) — the sender is cloned into the debouncer callback, the receiver lives in `App`
- The debouncer callback is `move |result: DebounceEventResult|`; filter events where `event.kind` is `EventKind::Create`, `EventKind::Modify`, or `EventKind::Remove`; check path extension with `path.extension() == Some(OsStr::new("json"))`
- Use `drop(tx.blocking_send(...))` inside the debouncer callback (it runs on a std thread, not an async context); ignore send errors (receiver dropped = app exited)
- Keep the `Watcher` struct alive in `App` for the full session lifetime — dropping it stops the OS watcher
- The runner channel drain pattern already in use (collect into `Vec` before processing) applies identically to the watcher channel — see CLAUDE.md note

## Success Metrics

- After editing a `prd.json` externally, the TUI reflects the change within 150 ms (50 ms debounce + one 100 ms poll tick + render)
- CPU usage during idle watching is negligible (OS-native push model, no polling)
- No TUI flicker or panic when files are rapidly created/modified/deleted

## Open Questions

- Should `reload_all()` also reload token data (`tokens.jsonl`) for the current plan? Not specified in scope but could be a natural extension once the token-tracking PRD is implemented.
- If the repo root has a very large number of `.json` files (e.g. `node_modules`), should we restrict the watch path to `.ralph/` instead of the full repo root? For now the filter-by-extension approach is sufficient, but a `.gitignore`-aware filter could be added later.
