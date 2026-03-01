# PRD: ralph-cli Token Tracking

## Introduction

Add token usage tracking to ralph-cli so users can see how many tokens each ralph loop iteration consumed, the total for a plan, and the running session total. Token data is read from state files written by the Claude Code CLI inside `.ralph/plans/<name>/` and displayed in the TUI without requiring API access.

## Goals

- Show per-iteration and per-plan token usage in the TUI
- Show a running session total across all plans
- Persist token data between sessions so historical costs are visible
- Keep implementation simple: read from files, not from API responses

## User Stories

### US-001: Token state file schema and writer

**Description:** As a developer, I need a defined token state file format written after each ralph iteration so the TUI has something to read.

**Acceptance Criteria:**

- [ ] Define `TokenRecord` struct: `{ iteration: u32, timestamp: String (ISO-8601), input_tokens: u64, output_tokens: u64, cache_read_tokens: u64, cache_write_tokens: u64 }`
- [ ] `TokenStore` struct in `src/ralph/tokens.rs` with `load(plan_dir: &Path) -> Vec<TokenRecord>` and `append(plan_dir: &Path, record: TokenRecord) -> Result<()>`
- [ ] Token records are stored in `.ralph/plans/<name>/tokens.jsonl` (newline-delimited JSON, one record per line)
- [ ] `load` returns empty vec if file does not exist (no error)
- [ ] `append` creates the file if it does not exist; appends without truncating existing records
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-002: Parse token usage from claude subprocess output

**Description:** As a developer, I need to extract token counts from the claude CLI output stream so I can populate `TokenRecord` without calling the API directly.

**Acceptance Criteria:**

- [ ] The runner module inspects each line of claude subprocess output for token usage patterns (e.g. lines matching `Tokens used:` or similar patterns from the claude CLI)
- [ ] When a token summary line is detected, parse input/output/cache token counts from it
- [ ] Parsed counts are accumulated during a single iteration and written as one `TokenRecord` on `RunnerEvent::Exited`
- [ ] If no token lines are found in a run (CLI version doesn't emit them), `TokenRecord` is written with all counts as 0 and a note in `notes` field: `"no token output detected"`
- [ ] The parsing is isolated in a `parse_token_line(line: &str) -> Option<TokenCounts>` function for testability
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-003: Per-plan token summary in the Stories panel

**Description:** As a user, I want to see the total tokens used for the active plan in the Stories panel so I can track cost per feature.

**Acceptance Criteria:**

- [ ] Stories panel footer (below the story list) shows: `Tokens: <N> input / <N> output  (<N> iterations)`
- [ ] Counts are the sum across all `TokenRecord` entries for that plan
- [ ] If no token data exists for the plan yet, shows: `Tokens: no data yet`
- [ ] Token counts display with thousands separators: `12,340` not `12340`
- [ ] Footer updates after each iteration completes (on plan reload)
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-004: Session total in the status bar

**Description:** As a user, I want to see the total tokens used in the current session (across all plans) in the status bar so I can track overall cost while the TUI is open.

**Acceptance Criteria:**

- [ ] Status bar shows session token total: `Session: <N>k tokens` (abbreviated, e.g. `12k`) appended to the right side
- [ ] Session total is the sum of tokens from all iterations that ran since the TUI was opened (in-memory only, not persisted)
- [ ] Session total updates after each iteration completes
- [ ] If no iterations have run yet this session, token display is omitted from the status bar (no clutter)
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-005: Token history view overlay

**Description:** As a user, I want to press `[T]` (shift+t) on a plan to see a per-iteration token breakdown so I can understand which stories cost the most.

**Acceptance Criteria:**

- [ ] Pressing `Shift+T` on a focused plan opens a full-screen overlay showing a table with columns: `Iter`, `Date`, `Input`, `Output`, `Cache Read`, `Cache Write`
- [ ] One row per `TokenRecord`, sorted by iteration ascending
- [ ] Footer row shows totals: `TOTAL  —  <sum>  <sum>  <sum>  <sum>`
- [ ] Any key closes the overlay and returns to the main TUI
- [ ] If no token data exists, shows: `No token data for this plan yet.`
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

## Functional Requirements

- FR-1: Token data is stored in `.ralph/plans/<name>/tokens.jsonl`; one JSON object per line
- FR-2: Token parsing is best-effort; missing or unparseable token output does not break the runner
- FR-3: Session total is in-memory only; it resets when the TUI is closed
- FR-4: Historical totals (per plan) persist across sessions via `tokens.jsonl`
- FR-5: Token display does not require API access — only the subprocess output and local files

## Non-Goals

- No cost calculation in dollars (token counts only)
- No alerts or limits when token usage exceeds a threshold
- No cross-repository session aggregation
- No token tracking for synthesis runs (ralph loop only)

## Technical Considerations

- The exact format of token summary lines in `claude` CLI output needs to be confirmed by running the CLI and inspecting output
- If the claude CLI does not emit parseable token data, a fallback approach (e.g. reading `~/.claude/logs/` or using `--output-format json`) may be needed — investigate before implementing
- `tokens.jsonl` format (newline-delimited JSON) is append-only and easy to parse incrementally

## Success Metrics

- After a full ralph loop, the user can see per-story and total token costs without checking any external dashboard
- Token data persists and is visible on next TUI launch

## Open Questions

- Does the `claude` CLI emit token usage to stdout/stderr? What is the exact format?
- Is there a `--output-format json` flag that includes token metadata per message?
- Should cache tokens be counted toward the displayed total or shown separately to avoid confusion?
