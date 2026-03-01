# PRD: ralph-cli PRD Editor (Form-Based)

## Introduction

Add a form-based PRD editor inside the TUI so users can create and edit user stories without manually editing `prd.json`. The editor presents structured fields (project name, branch, description, stories, acceptance criteria) and writes the result to disk in the correct `prd.json` format.

This replaces the `[e]` shortcut that opens `$EDITOR` for power users who prefer guided input over raw JSON editing.

## Goals

- Allow creating and editing plans via structured form fields, not raw JSON
- Keep stories small and well-formed by surfacing key fields inline
- Write valid `prd.json` on save without requiring the user to know the schema
- Be accessible alongside the existing `$EDITOR` escape hatch (both workflows coexist)

## User Stories

### US-001: Plan metadata form — edit project name, branch, and description

**Description:** As a user, I want to open a form that lets me edit the plan-level fields (project, branch, description) so I can set up a new plan without editing raw JSON.

**Acceptance Criteria:**

- [ ] Pressing `E` (shift+e) on a focused plan opens a full-screen form overlay (replacing the normal TUI)
- [ ] Form has three top fields: `Project:`, `Branch:`, `Description:` pre-populated from the current `prd.json` values
- [ ] Tab / Shift+Tab moves focus between fields
- [ ] Typing replaces field content character by character; Backspace removes last character
- [ ] `Save` action (Ctrl+S or a visible `[Ctrl+S] Save` hint) writes updated fields to `prd.json` and closes form
- [ ] Esc closes form without saving changes
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-002: Story list in form — navigate and select a story to edit

**Description:** As a user, I want to see a list of all stories in the editor so I can pick one to edit or add a new one.

**Acceptance Criteria:**

- [ ] Below the metadata fields, the form shows a scrollable list of stories: `[N] US-001: <title>` one per line
- [ ] Up/Down arrows or j/k navigate the story list
- [ ] Enter on a story opens the story detail form (US-003)
- [ ] Pressing `a` opens an empty story detail form for a new story
- [ ] Pressing `x` on a selected story prompts `Delete story US-XXX? [y/N]`; pressing `y` removes it from the list; any other key cancels
- [ ] Story list updates live after edits or additions
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-003: Story detail form — edit story fields inline

**Description:** As a user, I want a focused form for a single story where I can edit all its fields so I can write good acceptance criteria without leaving the TUI.

**Acceptance Criteria:**

- [ ] Story detail form shows fields: `ID:`, `Title:`, `Description:`, `Priority:`, `Acceptance Criteria:` (multi-line, one criterion per line)
- [ ] Tab / Shift+Tab moves between fields; Enter in the criteria field adds a new line below the cursor
- [ ] `x` on a focused criterion line deletes that line
- [ ] Ctrl+S saves the story back into the plan's `userStories` list and returns to the story list
- [ ] Esc discards changes and returns to the story list without saving
- [ ] ID field auto-fills as `US-NNN` (next available) for new stories; user can override
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-004: Persist form changes to prd.json

**Description:** As a developer, I need form saves to write valid `prd.json` to disk so the ralph agent can use the file without transformation.

**Acceptance Criteria:**

- [ ] Ctrl+S at any level of the form (metadata or story detail) triggers a full save of the current in-memory `PrdJson` state to `prd.json` via `Plan::save`
- [ ] Saved file is valid JSON and passes deserialization with the existing `PrdJson` struct
- [ ] After saving, the Stories panel in the main TUI reflects the updated story list immediately
- [ ] If `Plan::save` returns an error, show the error in the form's status line without closing the form
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-005: Validation commands field

**Description:** As a user, I want to edit the `validationCommands` list in the form so I can configure what ralph runs to validate each story.

**Acceptance Criteria:**

- [ ] Form metadata section includes a `Validation Commands:` multi-line field (one command per line)
- [ ] Pre-populated from `prd.json`'s `validationCommands` array
- [ ] Enter adds a new command line; `x` on a focused line deletes it
- [ ] Saved correctly as `validationCommands` array in `prd.json`
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

## Functional Requirements

- FR-1: The editor is opened with `Shift+E`; `e` still opens `$EDITOR` for raw JSON editing
- FR-2: In-memory state is a mutable copy of `PrdJson`; disk is only written on explicit save (Ctrl+S)
- FR-3: Closing the form without saving discards all in-memory changes — no auto-save
- FR-4: The editor must not corrupt `prd.json` — always serialize through the existing `Plan::save` method
- FR-5: Story IDs are strings; the editor does not enforce uniqueness but shows a warning if duplicates are detected on save

## Non-Goals

- No drag-and-drop story reordering (priority is edited as a number field)
- No rich text or markdown preview in criteria fields
- No undo/redo beyond the current session
- No multi-plan editing in one form

## Technical Considerations

- Use `tui-textarea` crate for multi-line text fields, or implement a simple line-buffer widget
- The form replaces the normal three-panel layout while open (full-screen modal approach)
- Ctrl+S must be intercepted before crossterm translates it to something else — test on macOS and Linux

## Success Metrics

- A user can create a complete `prd.json` with three stories without touching a text editor
- Form saves produce valid `prd.json` that the ralph agent can process without errors

## Open Questions

- Should `Shift+E` open the form or should there be a dedicated `[f]orm` keybinding to avoid conflicts?
- Should story reordering by priority number be validated (no duplicate priorities)?
