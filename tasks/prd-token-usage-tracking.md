# PRD: Token Usage Tracking

## Introduction

Track Claude API token usage (input, output, cache read, cache write tokens and estimated USD cost) on a per-user-story basis as ralph executes a workflow. Usage is parsed from the Claude CLI PTY output, displayed live in the runner tab status bar, and persisted to `usage.json` in the workflow directory after each story completes.

## Goals

- Parse token counts and estimated cost from the Claude CLI PTY output stream.
- Accumulate token data per user story in `RunnerTab`; reset accumulators when a new story begins.
- Show estimated cost of the current story live in the runner tab status bar (right side).
- Persist per-story and cumulative totals to `.ralph/workflows/<name>/usage.json` on each story completion.

## User Stories

### US-001: Add token accumulation fields to RunnerTab

**Description:** As a developer, I need `RunnerTab` to hold running token/cost totals for the current story so that the TUI can display live cost and the completion handler can save it.

**Acceptance Criteria:**

- [ ] Add the following fields to `RunnerTab` in `src/app.rs`:
  - `pub current_story_input_tokens: u64` — initialised to `0`
  - `pub current_story_output_tokens: u64` — initialised to `0`
  - `pub current_story_cache_read_tokens: u64` — initialised to `0`
  - `pub current_story_cache_write_tokens: u64` — initialised to `0`
  - `pub current_story_cost_usd: f64` — initialised to `0.0`
- [ ] All five fields are reset to `0`/`0.0` inside `start_runner()` (fresh start) and at the top of `spawn_next_iteration_at()` (before the new child spawns), so each story begins with a clean slate.
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-002: Parse token usage from PTY byte stream

**Description:** As a developer, I need the reader task to detect token/cost output from the Claude CLI and send it over the runner channel so the app can accumulate it.

**Acceptance Criteria:**

- [ ] Add a new variant to `RunnerEvent` in `src/ralph/runner.rs`:
  ```rust
  TokenUsage {
      input_tokens: u64,
      output_tokens: u64,
      cache_read_tokens: u64,
      cache_write_tokens: u64,
      cost_usd: f64,
  }
  ```
- [ ] In the `spawn_blocking` reader closure in `src/app.rs` (the same block that already detects `<promise>COMPLETE</promise>`), after converting the chunk to lossy UTF-8, scan for the Claude CLI cost line. The Claude CLI prints a summary line of the form:
  ```
  Cost: $0.0123 (1,234 input, 567 output, 890 cache read, 123 cache write tokens)
  ```
  Use a regex (add the `regex` crate, or use manual string parsing) to extract:
  - The dollar amount (f64 cost)
  - `input` token count (u64)
  - `output` token count (u64)
  - `cache read` token count (u64, may be absent — default 0)
  - `cache write` token count (u64, may be absent — default 0)
  Note: numbers in the CLI output may include commas as thousands separators; strip commas before parsing.
- [ ] When a match is found, send `RunnerEvent::TokenUsage { … }` over `tx_read` (before sending the `Bytes` event so the accumulator is updated before the next draw).
- [ ] In `drain_tab_channel` in `src/app.rs`, handle `Ok(RunnerEvent::TokenUsage { … })` by **adding** each field to the corresponding `current_story_*` field on `self.runner_tabs[tab_idx]`. Use saturating addition for integer fields.
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-003: Persist token usage to usage.json on story completion

**Description:** As a user, I want token usage for each completed story saved to `usage.json` in the workflow directory so I can review costs after a run.

**Acceptance Criteria:**

- [ ] Create `src/ralph/usage.rs` and register it in `src/ralph/mod.rs` as `pub mod usage;`.
- [ ] Define the following serialisable structs (all fields use camelCase via `#[serde(rename = "...")]` to match the existing JSON style in the project):
  ```rust
  #[derive(Debug, Clone, Default, Serialize, Deserialize)]
  pub struct TaskUsage {
      #[serde(rename = "inputTokens")]
      pub input_tokens: u64,
      #[serde(rename = "outputTokens")]
      pub output_tokens: u64,
      #[serde(rename = "cacheReadTokens")]
      pub cache_read_tokens: u64,
      #[serde(rename = "cacheWriteTokens")]
      pub cache_write_tokens: u64,
      #[serde(rename = "estimatedCostUsd")]
      pub estimated_cost_usd: f64,
  }

  #[derive(Debug, Clone, Default, Serialize, Deserialize)]
  pub struct UsageFile {
      /// Per-story token usage, keyed by task id (e.g. "US-001").
      pub tasks: std::collections::HashMap<String, TaskUsage>,
      /// Running total across all stories recorded so far.
      pub total: TaskUsage,
  }
  ```
- [ ] Implement `UsageFile::load(dir: &Path) -> Result<Self>` that reads and parses `usage.json` from `dir`. If the file does not exist, return `Ok(UsageFile::default())`.
- [ ] Implement `UsageFile::save(&self, dir: &Path) -> Result<()>` that writes pretty-printed JSON to `usage.json` in `dir`.
- [ ] Implement `UsageFile::record_story(&mut self, task_id: &str, usage: TaskUsage)` that:
  - Inserts (or replaces) `usage` into `self.tasks` under `task_id`.
  - Recomputes `self.total` as the sum across all entries in `self.tasks`.
- [ ] In the story-completion path inside `drain_tab_channel` in `src/app.rs` — immediately after the workflow is saved/reloaded and before `spawn_next_iteration_at` or state is set to `Done` — do the following:
  1. Build a `TaskUsage` from the five `current_story_*` fields on the tab.
  2. Load `UsageFile` from the workflow dir (tolerating missing file).
  3. Call `record_story` with the current `tab.current_task_id` (skip if `None`).
  4. Save `UsageFile` back to disk. Log or silently swallow errors (do not crash the app).
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-004: Display per-story cost in TUI status bar

**Description:** As a user, I want to see the estimated cost of the current story as it runs so I can monitor spend at a glance.

**Acceptance Criteria:**

- [ ] Modify `runner_tab_context` in `src/ui.rs` to accept the five accumulator fields from `RunnerTab` and append `  $X.XXXX` (4 decimal places) to the existing context string:
  - New format: `"{task_title}  {done}/{total} tasks  iter {n}  $0.0123"`
  - When in `Running` state: use `tab.current_story_cost_usd`.
  - When in `Done` state: read `usage.json` (via `UsageFile::load`) for the workflow and display the grand total `total.estimated_cost_usd`. If the file cannot be read, show `$?.????`.
- [ ] The cost string uses exactly 4 decimal places (e.g. `$0.0000`, `$0.0034`, `$1.2345`).
- [ ] The existing right-alignment and graceful truncation behaviour via `notification_right_spans` is preserved — no layout regressions at narrow terminal widths.
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

## Functional Requirements

- **FR-1:** `RunnerTab` accumulates token data for the active story; fields reset at the start of each new story (fresh start and auto-loop iterations).
- **FR-2:** The reader thread scans PTY output for Claude CLI cost lines and emits `RunnerEvent::TokenUsage`; the drain loop accumulates received events into `RunnerTab`.
- **FR-3:** On story completion, the accumulated `TaskUsage` is saved to `.ralph/workflows/<name>/usage.json` with per-story entries and a recomputed total.
- **FR-4:** `usage.json` is loaded on first access per run; if absent it is created fresh. The file is updated (not replaced) after each story — previous story entries are preserved.
- **FR-5:** The runner tab status bar right side shows the current story's running cost in `$X.XXXX` format while Running, and the workflow grand total when Done.
- **FR-6:** All token counts use `u64` (saturating addition); cost uses `f64`.
- **FR-7:** Numbers in the Claude CLI output may contain comma thousands-separators; these are stripped before parsing.

## Non-Goals

- No support for parsing token data from non-Claude CLI runners.
- No UI screen or separate view for historical usage; `usage.json` is the only historical record.
- No model-specific pricing table; cost is parsed directly from the Claude CLI output, not computed.
- No alerting or hard stops based on cost thresholds.
- No multi-session cost aggregation across separate ralph invocations (each run appends to the existing `usage.json`).

## Technical Considerations

- The `regex` crate should be evaluated first; if it pulls in too much compile-time weight, manual string parsing of the cost line is acceptable and preferred (the format is predictable).
- The cost line parsing lives in the same `spawn_blocking` closure that already scans for `<promise>COMPLETE</promise>`. Keep scanning logic co-located and clearly commented.
- `UsageFile::load` must tolerate a missing `usage.json` (return `Default::default()`) but should propagate genuine parse errors as `Err`.
- Disk writes in `drain_tab_channel` are synchronous; this is acceptable because the cost line appears at most once per story and the write is small.
- The `runner_tab_context` function currently calls `Workflow::load` on every draw frame to get `done/total` counts. Reuse that same load site — do not add a second `Workflow::load` call in the same function.
- For the Done state cost display, `UsageFile::load` is called in `runner_tab_context` (draw path); this is a small JSON read and acceptable for a per-frame call given the file is tiny. If it becomes a concern, cache it on `RunnerTab`.

## Success Metrics

- Token counts and cost appear in the status bar within the same render cycle as the Claude CLI outputs the cost line.
- `usage.json` is written correctly after each story and readable as valid JSON.
- No regressions in the existing TUI layout, auto-loop behaviour, or validation commands.

## Open Questions

- What is the exact format of the Claude CLI cost line? The PRD assumes `Cost: $X.XXXX (N,NNN input, N,NNN output, N,NNN cache read, N,NNN cache write tokens)`. The implementor should verify against the actual CLI output and adjust the parsing regex/logic accordingly. Cache read/write counts may be absent in some runs.
- Should the Done-state cost in the status bar reflect only stories completed in the current session, or the full `usage.json` total (which may include prior sessions)? Current answer: full `usage.json` total, since it is the most informative.
