# PRD: Getting Started Documentation

## Introduction

The `ralph-cli` README lacks an end-to-end walkthrough for new users. Someone who discovers the project today has no clear path from "I just cloned this" to "a Claude agent is running tasks in my repo". This PRD adds a **Getting Started** section to `README.md` that walks a contributor through every step: installing prerequisites, setting up the tool, creating a fresh git repo, writing a PRD with Claude, synthesizing it into a workflow, launching the TUI, and running the agent loop.

---

## Goals

- Give any developer with Rust and `just` installed a working end-to-end experience in under 10 minutes
- Document exactly which Claude skills are installed by `just set-resources` and what they do
- Make it clear that ralph-cli must be run inside a git repository
- Provide a copy-pasteable snippet to bootstrap a new sandbox git repo

---

## User Stories

### US-001: Add a "Getting Started" section to README.md

**Description:** As a developer who has just discovered ralph-cli, I want a step-by-step guide in the README so that I can go from zero to a running agent loop without guessing.

**Acceptance Criteria:**

- [ ] A new `## Getting Started` section is added to `README.md`, positioned before the existing `## Development` section
- [ ] The section opens with a one-paragraph "What is this?" explanation covering ralph-cli, the Ralph agent, and how they relate to Claude Code
- [ ] Step 1 documents prerequisites: Rust (edition 2024, toolchain ≥ 1.86) and `just`, with install hints for both
- [ ] Step 2 shows how to clone/checkout the repo: `git clone` command plus a note that contributors testing a PR should `git checkout <branch>`
- [ ] Step 3 shows `just set-resources` and explains what it does (copies `/prd` and `/prd-synth` skills and the `ralph` agent into `~/.claude/`)
- [ ] Step 4 shows `just install` and explains it installs the `ralph-cli` binary via `cargo install --path .`
- [ ] Step 5 provides a shell snippet to create a brand-new sandbox git repo to run workflows in (e.g. `mkdir my-project && cd my-project && git init && git commit --allow-empty -m "init"`)
- [ ] Step 6 instructs the user to open a Claude Code session inside that repo (`claude` CLI) and run `/prd <feature description>`, with the exact example from the feature request: `/prd We need a feature that shows token usage for ralph usage in this TUI`
- [ ] Step 7 instructs the user to run `/prd-synth` inside the same Claude session to synthesize the PRD into a `.ralph/workflows/<name>/prd.json` file
- [ ] Step 8 instructs the user to exit the Claude session and run `ralph-cli` from the repo root
- [ ] Step 9 instructs the user to press `r` on the selected workflow to start the agent loop and wait for tasks to complete
- [ ] The section includes a brief callout box / blockquote explaining that ralph-cli must always be run from inside a git repository (or a subdirectory of one)
- [ ] All shell commands are in fenced code blocks with the `sh` language tag
- [ ] `cargo fmt`, `cargo build`, and `cargo clippy -- -D warnings` all pass with no changes to Rust source

---

## Functional Requirements

- FR-1: The `## Getting Started` section must be self-contained — a reader should not need to read any other section first
- FR-2: The section must document that `just set-resources` installs two Claude Code skills (`/prd`, `/prd-synth`) and the `ralph` agent into `~/.claude/`
- FR-3: The git repo bootstrap snippet must produce a repo with at least one commit, because some git operations (e.g. branch detection) require a non-empty repo
- FR-4: The Claude session steps (US-001 steps 6–7) must note that the Claude session is opened in the **sandbox repo**, not in the ralph-cli repo itself
- FR-5: The section must not duplicate content already covered in other README sections; cross-link where appropriate (e.g. "See Keybindings for the full key reference")

---

## Non-Goals

- No changes to any Rust source files
- No documentation for the shell scripts in `scripts/ralph/` (legacy, already documented)
- No documentation for advanced use cases (multiple concurrent workflows, custom validation commands, etc.)
- No video or GIF walkthrough

---

## Technical Considerations

- Edit only `README.md`; no new files needed
- The new section goes between the existing `## What it does` / `## Workflow file layout` sections and the `## Development` section
- Keep Markdown formatting consistent with the rest of the file (pipe tables, fenced code blocks, `---` horizontal rules between sections)

---

## Success Metrics

- A developer unfamiliar with ralph-cli can follow the guide and reach a running agent loop without consulting any other document
- No open questions remain about which directory to run each command in

---

## Open Questions

None — all questions resolved:

- Do not repeat the upstream Ralph / Claude Code links in the Getting Started section; they already appear in the README title.
- Use `git commit --allow-empty -m "init"` for the bootstrap command (empty commits work for ralph's git-root detection).
