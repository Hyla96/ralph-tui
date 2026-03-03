# PRD: Fix Token Usage Tracking & Add Token Display to Workflow View

## Introduction

The token tracking feature was implemented (PRD `prd-token-usage-tracking.md`) but `usage.json` always shows zeros. The root cause is unknown — most likely the parser does not match what the Claude CLI actually prints in the PTY stream (wrong prefix, ANSI escape codes, or the cost line being split across 4096-byte PTY chunks). Once fixed, the UI should surface per-task and aggregate token counts in the Workflow tab and the runner tab. Cost/USD is not displayed anywhere.

## Goals

- Diagnose and fix the parser so `usage.json` accumulates real token counts.
- Show per-task token counts and aggregate totals in the Workflow tab's Tasks panel.
- Show both the current task's running token count and the session aggregate in the runner tab status bar.

## User Stories

---

### US-001: Diagnose the PTY token-line parser

**Description:** As a developer, I need to know exactly what text the Claude CLI writes to the PTY so I can fix the parser that is currently producing all-zero token counts.

**Acceptance Criteria:**

- [ ] Add support for a `RALPH_DEBUG_PTY=1` environment variable. When set, every UTF-8 chunk read from the PTY (after stripping ANSI escape sequences) is **appended** to `.ralph/pty-debug.log` in the current working directory. Each chunk is separated by a `---` delimiter line so chunks are distinguishable.
  - ANSI stripping: remove all sequences matching `\x1b\[[0-9;]*[a-zA-Z]` (and OSC `\x1b]...\x07` if easy to add).
  - This file is never read by the app; it is purely a debug artefact.
- [ ] Run ralph with `RALPH_DEBUG_PTY=1` against any small workflow so the Claude CLI executes at least one task. Inspect `.ralph/pty-debug.log` to find the actual token summary line format.
- [ ] Document the real format at the top of the `parse_cost_line` function as a comment (e.g. `// Actual observed format: "Total tokens: 1,234 input | 567 output"`).
- [ ] `cargo build` passes.
- [ ] `cargo clippy -- -D warnings` passes.

---

### US-002: Fix the token-line parser and handle chunk-splitting

**Description:** As a developer, I need the parser to reliably extract token data from the PTY stream so that `usage.json` reflects real values rather than zeros.

**Acceptance Criteria:**

- [ ] Update `parse_cost_line` in `src/app.rs` to match the actual format confirmed in US-001. Rename the function to `parse_token_line` if the token summary line no longer contains a cost prefix. Keep parsing `cost_usd` if it still appears in the same line (store it in `TaskUsage.estimated_cost_usd` for data completeness, but it is not displayed).
- [ ] Fix chunk-splitting: the PTY reader currently scans each 4096-byte chunk independently. If the token summary line spans two consecutive chunks, neither chunk matches. Fix by maintaining a **tail buffer** in the reader closure: keep the last ~512 bytes from the previous chunk and prepend them to the current chunk before scanning. The tail buffer is updated after every chunk read (`tail = last 512 bytes of current chunk`).
  - The tail buffer must not be sent as a `Bytes` event twice — only the real chunk bytes are forwarded.
- [ ] Strip ANSI escape sequences from the string before running the parser (reuse the same stripping logic as US-001).
- [ ] Write a unit test `parse_token_line_parses_actual_format` (in a `#[cfg(test)]` block at the bottom of `src/app.rs`) that asserts correct parsing for:
  1. The exact format observed in US-001 with typical values.
  2. A line where cache tokens are absent (should default to 0).
  3. Numbers with comma thousands-separators.
- [ ] Run ralph with a small workflow and verify `usage.json` now contains non-zero `inputTokens` and `outputTokens`.
- [ ] `cargo build` passes.
- [ ] `cargo clippy -- -D warnings` passes.

---

### US-003: Show per-task token counts in the Workflow Tasks panel

**Description:** As a user, I want to see how many tokens each completed task used in the task list on the Workflows tab so I can review consumption without opening `usage.json`.

**Acceptance Criteria:**

- [ ] In `draw_workflows_tab` in `src/ui.rs`, inside the `Some(workflow)` match arm, call `UsageFile::load(&workflow_dir)` once for the selected workflow. Treat a missing or unreadable file as `UsageFile::default()`.
  - `workflow_dir` is `app.store.workflow_dir(name)` where `name` is `app.workflows[selected_idx]`. Skip the load entirely if no workflow is selected.
- [ ] For each task whose `task.passes == true`, if a `TaskUsage` entry exists in `usage_file.tasks` keyed by `task.id`, append `  {N} tok` to the task line, where `N = entry.input_tokens + entry.output_tokens`, formatted with comma thousands-separators (e.g. `12,345 tok`). Use the same `DarkGray` style as the rest of the completed-task line.
  - Helper: add a free function `fn format_tokens(n: u64) -> String` in `src/ui.rs` that formats a `u64` with commas and appends ` tok` (e.g. `format_tokens(12345)` → `"12,345 tok"`).
- [ ] For tasks where `task.passes == false`, show no token suffix.
- [ ] Example rendered line:
  ```
  ✓ [P1] US-001: Add insert_mode field  12,345 tok
  ```
- [ ] `cargo build` passes.
- [ ] `cargo clippy -- -D warnings` passes.

---

### US-004: Show aggregate token count in the Tasks panel title

**Description:** As a user, I want to see the total tokens consumed by the selected workflow at a glance in the Tasks panel header.

**Acceptance Criteria:**

- [ ] Change the Tasks panel title from `"Tasks ({done}/{total})"` to `"Tasks ({done}/{total})  {N} tok"` when `usage_file.total.input_tokens + usage_file.total.output_tokens > 0`. Use the same `format_tokens` helper from US-003 applied to the sum `total.input_tokens + total.output_tokens`.
- [ ] When the total is zero (file absent, unreadable, or workflow never run), the title remains `"Tasks ({done}/{total})"` — no `0 tok` clutter.
- [ ] Reuse the `UsageFile` already loaded in US-003; do not call `UsageFile::load` a second time.
- [ ] Example title: `"Tasks (3/5)  45,678 tok"`.
- [ ] `cargo build` passes.
- [ ] `cargo clippy -- -D warnings` passes.

---

### US-005: Show session and current-task token counts in the runner tab status bar

**Description:** As a user, I want to see both the running token count of the current task and the total session token count in the runner status bar so I can monitor consumption at two levels simultaneously.

**Acceptance Criteria:**

- [ ] Remove the cost string (`$X.XXXX`) from `runner_tab_context` in `src/ui.rs`. Replace it with token counts.
- [ ] When in `Running` state, show two token figures:
  - **Task tokens**: `tab.current_story_input_tokens + tab.current_story_output_tokens` — tokens consumed by the current story so far.
  - **Session tokens**: `usage_file.total.(input+output) + current task tokens` — completed stories from `usage.json` plus the current story's running total. Load `UsageFile` with the `workflow_dir` variable already in scope; on load failure, omit the session figure.
  - New Running format: `"{task_title}  {done}/{total} tasks  iter {n}  task: 1,234 tok  session: 45,678 tok"`.
  - If `UsageFile` load fails, fall back to: `"{task_title}  {done}/{total} tasks  iter {n}  task: 1,234 tok"`.
- [ ] When in `Done` state, show only the session total from `usage.json` (the last task is already included):
  - Format: `"{task_title}  {done}/{total} tasks  iter {n}  session: 45,678 tok"`.
  - If `UsageFile` load fails, show `"session: ? tok"`.
- [ ] Use `format_tokens` (defined in US-003) for all token formatting.
- [ ] Also remove `current_story_cost_usd` from `RunnerTab` and its reset/accumulation sites in `src/app.rs` if the field is no longer used in any display path. If it is still written to `TaskUsage.estimated_cost_usd` for persistence, keep it; just stop displaying it.
- [ ] `cargo build` passes.
- [ ] `cargo clippy -- -D warnings` passes.

---

## Functional Requirements

- **FR-1:** `RALPH_DEBUG_PTY=1` writes ANSI-stripped PTY chunks to `.ralph/pty-debug.log`; never read by the app.
- **FR-2:** The token-line parser matches the actual Claude CLI output format confirmed via the debug log.
- **FR-3:** A tail buffer prevents token summary lines split across consecutive 4096-byte PTY chunks from being missed.
- **FR-4:** ANSI escape sequences are stripped before parsing; raw bytes are still forwarded to the vt100 parser unchanged.
- **FR-5:** Completed tasks in the Workflow Tasks panel show `{N} tok` (input + output) when a `TaskUsage` entry exists.
- **FR-6:** The Tasks panel title shows the workflow's total token count when non-zero.
- **FR-7:** The runner status bar shows `task: N tok  session: N tok` while Running; `session: N tok` when Done. No cost/USD is shown anywhere.
- **FR-8:** `UsageFile::load` is called at most once per draw frame per panel.

## Non-Goals

- No USD cost display anywhere in the UI (cost may still be stored in `usage.json` for future use, but is not shown).
- No changes to the `UsageFile` or `TaskUsage` JSON schemas.
- No changes to when/how `usage.json` is written.
- No persistent UI state for `RALPH_DEBUG_PTY`; it is a developer-only flag.
- No separate cache token display (cache read/write tokens are tracked in the file but not shown in the UI).
- No sorting or filtering of tasks by token count.

## Technical Considerations

- The `RALPH_DEBUG_PTY=1` check: cache `std::env::var("RALPH_DEBUG_PTY").is_ok()` once at the start of the reader closure, not per-chunk.
- Tail buffer: `let mut tail: Vec<u8> = Vec::new();` in the reader closure. After each chunk, update: `tail = chunk[chunk.len().saturating_sub(512)..].to_vec()`. Before scanning, operate on `tail + chunk` (as a combined string); only forward the original `chunk` bytes as `Bytes`.
- `format_tokens`: format with commas using a simple loop or `format!("{}", n)` + manual grouping. Example: 12345 → `"12,345 tok"`. A simple approach: format to string, then insert commas every 3 digits from the right.
- `UsageFile::load` in the draw path is per-frame but the file is tiny (< 1 KB); acceptable as-is.

## Success Metrics

- After running a workflow, `usage.json` contains non-zero `inputTokens` and `outputTokens`.
- Completed tasks in the Workflows tab show a non-zero `{N} tok` suffix.
- The Tasks panel title shows a non-zero token total for any workflow that has been run.
- The runner tab status bar shows `task: N tok  session: N tok` while running, and `session: N tok` when done.
- No regressions in the existing layout, auto-loop behaviour, or validation commands.

## Open Questions

- What is the exact Claude CLI token summary line format? (US-001 answers this; US-002 acts on it.)
- Should cache read/write tokens be included in the displayed totals? Current answer: no — show only input + output, which represents the "billed" tokens. Cache tokens are preserved in `usage.json` for completeness.
