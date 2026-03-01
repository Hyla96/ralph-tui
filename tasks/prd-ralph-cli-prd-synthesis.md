# PRD: ralph-cli PRD Synthesis

## Introduction

Add the ability to trigger PRD synthesis from within the TUI. Synthesis converts a human-readable markdown PRD file (stored in `.ralph/plans/<name>/prd-source.md` or an imported file) into a valid `prd.json` by invoking the `prd-synth` Claude Code skill. The output streams live into the log panel, and the resulting `prd.json` is reloaded automatically.

## Goals

- Allow users to import a markdown PRD and synthesize it into `prd.json` without leaving the TUI
- Stream synthesis output live so the user can see what claude is doing
- Auto-reload the plan after synthesis so the Stories panel updates immediately

## User Stories

### US-001: Import markdown PRD file into a plan

**Description:** As a user, I want to attach a markdown PRD file to a plan so that synthesis has a source document to work from.

**Acceptance Criteria:**

- [ ] Pressing `i` on a focused plan opens a file path input overlay: `Import PRD file: <cursor>`
- [ ] User types or pastes an absolute or relative file path and presses Enter
- [ ] On Enter, validate the path exists and is a `.md` file; show inline error if not: `File not found` or `Not a .md file`
- [ ] On success, copy the file to `.ralph/plans/<name>/prd-source.md` and close overlay
- [ ] If `prd-source.md` already exists, show: `Overwrite existing prd-source.md? [y/N]`; pressing `y` overwrites, any other key cancels
- [ ] Status bar shows `PRD imported` briefly after success
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-002: Trigger synthesis — spawn claude with prd-synth skill

**Description:** As a user, I want to press `[S]` (shift+s) on a plan that has a `prd-source.md` to run the prd-synth skill and generate `prd.json`, with output streaming live in the log panel.

**Acceptance Criteria:**

- [ ] Pressing `Shift+S` on a plan with `prd-source.md` present spawns: `claude --allowedTools "Read,Write,Edit" "$(cat .ralph/plans/<name>/prd-source.md)"` piped through the `prd-synth` skill invocation (exact command TBD based on how skills are invoked)
- [ ] If `prd-source.md` does not exist, shows error in status bar: `No prd-source.md — press [i] to import`
- [ ] Subprocess output streams line-by-line into the Log panel (same mechanism as the ralph runner)
- [ ] App enters a `Synthesizing` state; status bar shows `[s]top  Synthesizing…`; `r` is disabled while synthesizing
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-003: Post-synthesis reload and status

**Description:** As a user, I want the TUI to reload the plan and show updated stories automatically after synthesis completes so I can immediately start the ralph loop.

**Acceptance Criteria:**

- [ ] When the synthesis subprocess exits with code 0, reload the plan via `Plan::load` and re-render the Stories panel
- [ ] If subprocess exits with non-zero code, show in status bar: `Synthesis failed (exit N) — check log`; do not overwrite existing `prd.json`
- [ ] After successful synthesis, `AppState` returns to `Idle` and the status bar shows `Synthesis complete`
- [ ] If the synthesized output does not produce a valid `prd.json` (parse error), show: `prd.json invalid after synthesis — check log`
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-004: Show synthesis source status in plans list

**Description:** As a user, I want to see at a glance whether a plan has a source PRD attached so I know which plans are ready for synthesis.

**Acceptance Criteria:**

- [ ] Plans panel renders a small indicator next to each plan name: `[src]` if `prd-source.md` exists, nothing if it doesn't
- [ ] Example: `my-feature [src]` vs `another-feature`
- [ ] Indicator is recalculated each time the plans list is refreshed
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

## Functional Requirements

- FR-1: Synthesis is triggered with `Shift+S`; `r` (run ralph loop) is disabled during synthesis
- FR-2: The synthesis subprocess writes `prd.json` directly into `.ralph/plans/<name>/`; the TUI reloads after exit
- FR-3: `prd-source.md` is the synthesis input; it is never modified by the synthesis step
- FR-4: Log buffer from synthesis is separate from the ralph loop log (or at minimum clearly labeled in the log panel)
- FR-5: The `Synthesizing` app state is added to the `AppState` enum alongside `Idle`, `Running`, `Complete`

## Non-Goals

- No editing of `prd-source.md` inside the TUI (use `e` to open in `$EDITOR`)
- No diff view between old and new `prd.json` after synthesis
- No multi-file PRD import (one source file per plan)

## Technical Considerations

- The exact `claude` CLI invocation for running a skill needs to be confirmed — test manually before implementing
- Synthesis may take 30–120 seconds; the TUI must remain responsive and show streaming output
- If synthesis generates output to a different path (not `prd.json` in plan dir), a post-processing step may be needed to move the file

## Success Metrics

- User can go from a markdown PRD to a running ralph loop without opening any external tool
- Synthesis failure is clearly surfaced in the log panel with actionable output

## Open Questions

- What is the exact `claude` CLI command for invoking the `prd-synth` skill on a file?
- Should synthesis auto-archive an existing `prd.json` before overwriting (same logic as the bash scripts)?
