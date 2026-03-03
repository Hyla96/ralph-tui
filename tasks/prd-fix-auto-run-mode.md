# PRD: Fix Auto-Run Mode

## Introduction

The auto-run mode (toggled with `a` in a runner tab) is supposed to automatically cycle through workflow tasks: when the agent prints `<promise>COMPLETE</promise>`, the runner should stop the current process and spawn the next iteration for the next pending task. Currently, the COMPLETE sentinel is never detected because the raw PTY output contains ANSI escape codes that break the substring match. Additionally, the `.complete` file written by the agent is never read by the TUI and should be removed. Finally, the auto-run failure behavior should be changed so that failures prompt the user instead of auto-retrying.

## Goals

- Fix COMPLETE sentinel detection so auto-run correctly cycles through tasks
- Remove the unused `.complete` file mechanism from agent instructions
- Change auto-run failure behavior: only auto-continue on success, prompt on failure

## User Stories

### US-001: Fix COMPLETE sentinel detection in PTY output

**Description:** As a user, I want auto-run mode to detect the COMPLETE sentinel reliably so that the runner automatically cycles to the next task.

**Acceptance Criteria:**

- [ ] Strip ANSI escape codes from PTY output before checking for `<promise>COMPLETE</promise>` in the reader thread (`app.rs` ~line 280)
- [ ] The existing `strip_ansi()` helper (already used for debug logging and token parsing) should be reused for this
- [ ] Also check the combined tail+chunk buffer (not just the current chunk) to handle the rare case where the sentinel is split across two 4096-byte reads
- [ ] When COMPLETE is detected with auto-run ON, the current runner process is killed and the next iteration is spawned (same effect as manual stop + continue)
- [ ] `just check` passes (cargo build + clippy)

### US-002: Kill old process before spawning next iteration

**Description:** As a user, I want the old runner process to be explicitly killed when auto-continuing, so that orphaned processes don't accumulate.

**Acceptance Criteria:**

- [ ] When `drain_tab_channel` handles `complete=true` with `auto_continue=true`, send the kill signal through the existing `runner_kill_tx` before calling `spawn_next_iteration_at`
- [ ] This mirrors the `stop_runner()` behavior: take the kill_tx, send `()`, then spawn next
- [ ] Verify the old process is terminated (not just orphaned with dropped channels)
- [ ] `just check` passes

### US-003: Remove `.complete` file mechanism

**Description:** As a developer, I want to remove the unused `.complete` file so the codebase has a single, clear completion signal (`<promise>COMPLETE</promise>` sentinel).

**Acceptance Criteria:**

- [ ] Remove the `.complete` file write instruction from `resources/agents/ralph.md` (line 38: `printf '0' > "$RALPH_PLAN_DIR/.complete"`)
- [ ] Search the entire codebase for any other references to `.complete` and remove them
- [ ] The agent instructions should only mention outputting `<promise>COMPLETE</promise>` as the completion signal
- [ ] `just check` passes

### US-004: Change auto-run failure behavior to prompt on failure

**Description:** As a user, I want auto-run to only auto-continue on success (COMPLETE detected or exit code 0), and show the ContinuePrompt dialog on failure, so I can inspect failures before deciding to retry.

**Acceptance Criteria:**

- [ ] When `auto_continue=true` and the runner exits WITHOUT the COMPLETE sentinel and with a non-zero exit code, show `Dialog::ContinuePrompt` instead of auto-retrying
- [ ] When `auto_continue=true` and the runner exits with COMPLETE sentinel OR exit code 0, auto-continue to the next task as before
- [ ] The current auto-retry-on-failure logic (lines ~1983-1993 in `drain_tab_channel`) should be replaced with the ContinuePrompt path
- [ ] `just check` passes

## Functional Requirements

- FR-1: ANSI escape codes must be stripped before scanning for `<promise>COMPLETE</promise>` in the PTY reader thread
- FR-2: The sentinel scan must use the combined tail+chunk buffer to handle cross-chunk splits
- FR-3: When auto-continuing, the old runner process must be explicitly killed via `runner_kill_tx` before spawning the next iteration
- FR-4: The `.complete` file write must be removed from `resources/agents/ralph.md`
- FR-5: Auto-run with failure (no COMPLETE, non-zero exit) must show ContinuePrompt, not auto-retry
- FR-6: Auto-run with success (COMPLETE detected or exit code 0) must auto-continue to the next pending task

## Non-Goals

- No changes to the manual (non-auto) flow — ContinuePrompt behavior stays the same
- No changes to the MAX_ITERATIONS cap — it still applies
- No changes to the `a` keybinding or status bar display
- No file-based completion detection (`.complete` file polling)

## Technical Considerations

- The `strip_ansi()` function already exists in `app.rs` and is used for debug logging and token line parsing — reuse it
- The tail buffer (512 bytes from previous chunk) is already maintained for token line parsing — extend its use to COMPLETE detection
- The kill signal flow already exists in `stop_runner()` — the pattern of `kill_tx.take()` + `send(())` should be replicated
- `spawn_next_iteration_at` already handles channel replacement correctly; the key addition is sending the kill signal first

## Success Metrics

- Auto-run mode reliably detects COMPLETE and cycles to the next task without manual intervention
- No orphaned claude processes left running after auto-continue
- Failures pause for user confirmation instead of silently retrying

## Open Questions

- None — requirements are clear from user feedback.
