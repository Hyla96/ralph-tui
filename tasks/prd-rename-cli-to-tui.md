# PRD: Rename ralph-cli to ralph-tui

## Introduction

Rename the entire project from `ralph-cli` to `ralph-tui` to better reflect that the project is a **terminal user interface (TUI)** application rather than a traditional CLI tool. This is a comprehensive rebrand affecting the crate name, binary name, all code references, and all documentation. This is a breaking change for users but necessary to accurately represent the project's identity and focus.

## Goals

- Rename the Cargo crate from `ralph_cli` to `ralph_tui` (breaking change)
- Rename the binary from `ralph-cli` to `ralph-tui`
- Update all code references, comments, and strings mentioning "cli" to "tui"
- Update all documentation (README, CLAUDE.md, examples, etc.)
- Ensure the project builds, tests pass, and lints are clean after rename
- Update GitHub repository name (if applicable)
- Provide clear naming conventions going forward

## User Stories

### US-001: Rename Cargo crate and binary

**Description:** As a developer, I need the Cargo.toml to reflect the new project name so that builds and dependencies are correct.

**Acceptance Criteria:**

- [ ] Rename `name = "ralph_cli"` to `name = "ralph_tui"` in Cargo.toml
- [ ] Update `[[bin]]` name from `"ralph-cli"` to `"ralph-tui"` (if present)
- [ ] Update `[package]` description to mention TUI instead of CLI
- [ ] `cargo build` succeeds without warnings
- [ ] `cargo test` passes
- [ ] `just check` passes (cargo build + cargo clippy -- -D warnings)

### US-002: Update source code module names and paths

**Description:** As a developer, I need the source code structure to reflect the new name so that imports and module organization are clear.

**Acceptance Criteria:**

- [ ] Rename `src/ralph_cli/` directory to `src/ralph_tui/` (if it exists)
- [ ] Update all `mod ralph_cli` declarations to `mod ralph_tui`
- [ ] Update all `use ralph_cli::` imports to `use ralph_tui::`
- [ ] Update all `use crate::ralph_cli::` to `use crate::ralph_tui::`
- [ ] Search for and replace "ralph_cli" with "ralph_tui" in all Rust source files
- [ ] `just check` passes

### US-003: Update comments and documentation strings in code

**Description:** As a developer, I need code comments and docstrings to reference TUI instead of CLI for clarity and consistency.

**Acceptance Criteria:**

- [ ] Search for "ralph-cli" and "ralph_cli" in all `.rs` files
- [ ] Replace with "ralph-tui" and "ralph_tui" respectively in comments, doc comments, and string literals
- [ ] Replace generic "CLI" references with "TUI" where they describe the project's nature
- [ ] Preserve "CLI" if it refers to general command-line concepts (e.g., "CLI arguments" as a technical term)
- [ ] `just check` passes

### US-004: Update README and main documentation

**Description:** As a user, I need the README to accurately describe the project as a TUI application.

**Acceptance Criteria:**

- [ ] Update README.md title/header to reference "ralph-tui"
- [ ] Update all mentions of "ralph-cli" to "ralph-tui" in README
- [ ] Update installation instructions to use `ralph-tui` binary name
- [ ] Update any usage examples to reference the new binary name
- [ ] Update project description paragraph to emphasize TUI focus
- [ ] Verify links and references still work or are updated

### US-005: Update CLAUDE.md project instructions

**Description:** As a developer, I need the project instructions file to reference the correct project name.

**Acceptance Criteria:**

- [ ] Update the `# ralph-cli` heading to `# ralph-tui`
- [ ] Update any references to "ralph-cli" in commands or examples
- [ ] Ensure all recipes and build instructions are still accurate
- [ ] Verify the file is valid markdown

### US-006: Update all test files and examples

**Description:** As a developer, I need test files and examples to reference the correct crate and binary name.

**Acceptance Criteria:**

- [ ] Search for "ralph_cli" and "ralph-cli" in all test files
- [ ] Update imports in test files from `ralph_cli::` to `ralph_tui::`
- [ ] Update any hardcoded binary names in tests (e.g., running `./ralph-cli` → `./ralph-tui`)
- [ ] Update example files or scripts to use the new binary name
- [ ] `cargo test` passes with all tests green
- [ ] `just check` passes

### US-007: Update Cargo.lock and dependency references

**Description:** As a developer, I need any lock files or dependency configurations to reflect the renamed crate.

**Acceptance Criteria:**

- [ ] `cargo update` or `cargo build` regenerates Cargo.lock with new crate name
- [ ] Verify Cargo.lock has `name = "ralph_tui"` entries
- [ ] No stale "ralph_cli" references in Cargo.lock
- [ ] `cargo build --release` succeeds

### US-008: Update GitHub repository name and documentation links

**Description:** As a user, I need the repository to be discoverable under the new name.

**Acceptance Criteria:**

- [ ] Rename GitHub repository from `ralph-cli` to `ralph-tui` (if applicable)
- [ ] Update repository description on GitHub to mention TUI
- [ ] If any docs point to the old repo URL, update them
- [ ] Update any CI/CD configuration files that reference the repo name

### US-009: Update any CI/CD and build configuration files

**Description:** As a developer, I need CI/CD pipelines and build configs to work with the new crate name.

**Acceptance Criteria:**

- [ ] Search for "ralph_cli" and "ralph-cli" in CI files (.github/workflows, etc.)
- [ ] Update artifact names, binary paths, and crate references
- [ ] Update any build scripts that reference the old name
- [ ] CI/CD passes with new configuration

### US-010: Verify and document the breaking change

**Description:** As a maintainer, I need to communicate this breaking change to users.

**Acceptance Criteria:**

- [ ] Add a CHANGELOG entry noting the breaking change
- [ ] Document migration path for users (old: `ralph-cli`, new: `ralph-tui`)
- [ ] If applicable, tag this as a major version bump (semver)
- [ ] Update any installation or getting-started documentation with the new name

## Functional Requirements

- FR-1: The Cargo crate must be renamed from `ralph_cli` to `ralph_tui` in all configuration files
- FR-2: The binary executable must be named `ralph-tui` instead of `ralph-cli`
- FR-3: All Rust source code must use `mod ralph_tui` and `use ralph_tui::` instead of CLI equivalents
- FR-4: All code comments, docstrings, and string literals must reference "TUI" and "ralph-tui" appropriately
- FR-5: README, CLAUDE.md, and all user-facing documentation must reference the new project name
- FR-6: All tests must pass with imports and references updated to the new crate name
- FR-7: The project must build successfully with `cargo build` and `cargo build --release`
- FR-8: All clippy lints must pass with `cargo clippy -- -D warnings`
- FR-9: GitHub repository (if public) must be renamed to reflect the new project name
- FR-10: CI/CD pipelines must be updated to build and test the renamed crate successfully

## Non-Goals

- Creating new features or functionality during the rename
- Changing the project's core architecture or structure
- Updating dependencies or versions (unless required for the rename)
- Rewriting documentation from scratch (only rename existing documentation)
- Creating migration tools for users (they must update their installation manually)

## Technical Considerations

- **Breaking Change:** This is a major version bump. Users will need to reinstall with the new binary name.
- **Import Paths:** Any downstream crates that depend on `ralph_cli` will need to update to `ralph_tui`
- **Binary Name:** The binary path changes from `./target/debug/ralph-cli` to `./target/debug/ralph-tui`
- **Git History:** The rename will show as deletions and additions in git diff (this is expected)
- **Directory Structure:** If there's a `src/ralph_cli/` directory, it must be renamed to `src/ralph_tui/`
- **Cargo Metadata:** Update package name and binary name consistently in Cargo.toml

## Success Metrics

- All 10 user stories completed and verified
- `just check` passes (cargo build + clippy with -D warnings)
- `cargo test` passes with all tests green
- `cargo build --release` succeeds without warnings
- Zero references to "ralph_cli" or "ralph-cli" in source code (except in git history or comments explaining the rename)
- README and CLAUDE.md accurately describe the project as "ralph-tui"
- GitHub repository renamed (if applicable)

## Open Questions

- Should we maintain a compatibility alias or migration guide for users upgrading?
- Are there any external resources (blogs, docs, wikis) that reference `ralph-cli` that need updating?
- Should the version number be bumped (e.g., to 1.0.0 or 2.0.0) to mark this breaking change?
