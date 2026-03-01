# PRD: Agents & Skills Directory Management

## Introduction

Bundle the ralph agent definition (`agents/ralph.md`) and relevant skill definitions (`skills/prd/`, `skills/prd-synth/`) directly into the ralph-cli repository, and expose a TUI screen to manage which ones are installed into `~/.claude/`. This lets the repo be the source of truth for the agent/skill definitions used with the CLI, and gives users a UI to install or remove them individually.

## Goals

- Store the ralph agent and relevant skills under version control in the project
- Fix the ralph agent to work with ralph-cli's actual plan structure (`.ralph/plans/<name>/`)
- Improve the ralph agent based on learnings from the ralph-cli-core build
- Provide a TUI screen to list agents/skills and install/uninstall them to `~/.claude/`

## User Stories

### US-001: Create agents/ and skills/ directory structure

**Description:** As a developer, I want the ralph agent and ralph-relevant skills committed to the repo so they are versioned alongside the CLI that uses them.

**Acceptance Criteria:**

- [ ] `agents/ralph.md` exists — copied verbatim from `~/.claude/agents/ralph.md`
- [ ] `skills/prd/SKILL.md` exists — copied verbatim from `~/.claude/skills/prd/SKILL.md`
- [ ] `skills/prd-synth/SKILL.md` exists — copied verbatim from `~/.claude/skills/prd-synth/SKILL.md`
- [ ] `agents/` and `skills/` are NOT in `.gitignore` (they must be tracked)
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-002: Improve ralph agent for ralph-cli

**Description:** As ralph (the agent), I want my instructions to accurately reflect how ralph-cli manages plans so I can find `prd.json` and `progress.txt` without guessing the wrong path.

**Context for implementer:** The CLI spawns `claude --agent ralph` with `RALPH_PLAN_DIR` set to the active plan directory (e.g. `.ralph/plans/ralph-cli-core/`). The current `agents/ralph.md` still refers to `scripts/ralph/` which is wrong. The "never commit" rule also needs updating.

**Changes to make in `agents/ralph.md`:**

1. **Step 2** — change `prd.json (in scripts/ralph/)` to `prd.json` located at `$RALPH_PLAN_DIR/prd.json`
2. **Step 3** — change `progress.txt (in scripts/ralph/)` to `$RALPH_PLAN_DIR/progress.txt`
3. **Progress Report Format** — change `APPEND to scripts/ralph/progress.txt` to `APPEND to $RALPH_PLAN_DIR/progress.txt`
4. **Codebase Patterns section** — change reference from `progress.txt` in scripts/ralph to `$RALPH_PLAN_DIR/progress.txt`
5. **Rules section** — change "Never commit `prd.json`, `progress.txt`, or any files in `scripts/ralph/`" to "Never commit `prd.json` or `progress.txt` — these live under `.ralph/plans/` which is gitignored"
6. **Workflow Step 5** — clarify that the branch to use is `prd.branchName` and the agent should read it from `$RALPH_PLAN_DIR/prd.json`
7. **Add a new opening section** explaining the env var: "The CLI sets `RALPH_PLAN_DIR` env var to the active plan directory before spawning this agent. All plan files (`prd.json`, `progress.txt`) are located there."

**Acceptance Criteria:**

- [ ] `agents/ralph.md` no longer references `scripts/ralph/` anywhere
- [ ] `agents/ralph.md` references `$RALPH_PLAN_DIR/prd.json` and `$RALPH_PLAN_DIR/progress.txt`
- [ ] The rules section correctly states `.ralph/plans/` is gitignored (not `scripts/ralph/`)
- [ ] An introductory note explains `RALPH_PLAN_DIR` at the top of the Workflow section
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### US-003: TUI Agents & Skills management screen

**Description:** As a user, I want to open an "Agents & Skills" screen from the main TUI view to see which agents and skills are available in the project and whether they're installed in `~/.claude/`, and install or remove them with a keypress.

**Screen behavior:**

- Accessible from the main view via the `a` keybinding
- Press `Esc` or `q` to return to the main view
- Two sections in the list: **Agents** (from `agents/`) and **Skills** (from `skills/`)
- Each entry shows: name, type, and install status (`[installed]` or `[not installed]`)
- Arrow keys navigate the list
- `i` installs the selected item: copies the file(s) to `~/.claude/agents/<name>.md` or `~/.claude/skills/<name>/SKILL.md` (creates target dirs if needed)
- `u` uninstalls the selected item: removes the file from `~/.claude/` (agents: remove the `.md` file; skills: remove the entire `<name>/` dir)
- Status bar at the bottom shows: `[a] install  [u] uninstall  [Esc] back`
- Install/uninstall errors are surfaced in a one-line error message below the list

**Discovery logic:**

- Agents: any `.md` file directly under `agents/`
- Skills: any directory directly under `skills/` that contains a `SKILL.md`
- Install status: check if corresponding file/dir exists in `~/.claude/agents/` or `~/.claude/skills/`

**Acceptance Criteria:**

- [ ] `a` keybinding from main view opens the Agents & Skills screen
- [ ] Screen lists all agents found in `agents/` with install status
- [ ] Screen lists all skills found in `skills/` with install status
- [ ] `i` copies the selected agent/skill to `~/.claude/` and refreshes install status
- [ ] `u` removes the selected agent/skill from `~/.claude/` and refreshes install status
- [ ] `Esc` or `q` returns to the main view
- [ ] Status bar shows available keybindings
- [ ] If `agents/` or `skills/` dir doesn't exist, the screen shows an empty state message instead of panicking
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

## Functional Requirements

- FR-1: `agents/ralph.md` and `skills/prd/SKILL.md`, `skills/prd-synth/SKILL.md` are committed to the repo
- FR-2: `agents/ralph.md` reads `prd.json` and `progress.txt` from `$RALPH_PLAN_DIR`
- FR-3: The TUI exposes an Agents & Skills screen via `a` from the main view
- FR-4: The screen discovers agents and skills from the project's `agents/` and `skills/` directories
- FR-5: Install copies files to `~/.claude/`; uninstall removes them
- FR-6: Install/uninstall refreshes the displayed status immediately

## Non-Goals

- No syncing or diffing between project files and `~/.claude/` (install is a one-way copy)
- No support for nested skill directories or multi-file agents beyond the current conventions
- No watching `~/.claude/` for external changes
- No editing agents/skills from within the TUI (use `$EDITOR` for that)
- No bulk install-all / uninstall-all command

## Technical Considerations

- `agents/` and `skills/` paths are resolved relative to the project root (same dir as `.ralph/`)
- The `Store` struct already tracks the project root (`store.root_dir()` or equivalent); use that as the base path
- `~/.claude/` should be resolved via `dirs::home_dir()` or `std::env::var("HOME")` — add `dirs` crate if not already present
- The Agents & Skills screen is a new `AppMode` enum variant (follow the same pattern as existing modes)
- File copy is synchronous (no tokio needed — files are tiny); use `std::fs::copy` and `std::fs::remove_file`/`std::fs::remove_dir_all`

## Success Metrics

- Running `ralph` with any plan correctly finds `prd.json` via `$RALPH_PLAN_DIR`
- User can install both the ralph agent and skills in under 5 keypresses from the TUI
- No panics when `agents/` or `skills/` dirs are missing

## Open Questions

- Should `i` on an already-installed item overwrite silently, or ask for confirmation? (Proposed: overwrite silently — it's a simple file copy and the source is the truth)
- Should the screen show a diff/warning if the installed version differs from the project version? (Proposed: out of scope for now — non-goal above)
