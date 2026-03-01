# PRD: Rename Story → Task and Plan → Workflow

## Introduction

Rename the two core domain concepts throughout the codebase and on-disk data:

- **"Story"** (and `UserStory`) becomes **"Task"**
- **"Plan"** (and the `Plan` struct) becomes **"Workflow"**

This touches Rust structs, field names, method names, JSON serde keys, UI labels, dialog variants, and the on-disk directory layout (`.ralph/plans/` → `.ralph/workflows/`) plus all existing `prd.json` files.

## Goals

- All user-visible labels say "Task" / "Workflow" instead of "Story" / "Plan"
- Rust identifiers consistently use `Task` / `Workflow` terminology
- The JSON schema uses `"tasks"` instead of `"userStories"`
- The on-disk layout uses `.ralph/workflows/` instead of `.ralph/plans/`
- All existing `.ralph/**/prd.json` files are migrated in-place
- `cargo build` and `cargo clippy -- -D warnings` pass after each task

## User Tasks

---

### T-001: Rename `UserStory` → `Task` in plan.rs and update all call sites

**Description:** As a developer, I need the `UserStory` struct and all references to it renamed to `Task` so the codebase uses consistent terminology.

**Acceptance Criteria:**

- [ ] `src/ralph/plan.rs`: struct `UserStory` renamed to `Task`
- [ ] `src/ralph/plan.rs`: field `pub user_stories: Vec<UserStory>` on `PrdJson` renamed to `pub tasks: Vec<Task>`
- [ ] `src/ralph/plan.rs`: serde rename attribute updated from `#[serde(rename = "userStories")]` to `#[serde(rename = "tasks")]`
- [ ] `src/ralph/plan.rs`: method `next_story()` renamed to `next_task()`, return type updated to `Option<&Task>`
- [ ] `src/ralph/plan.rs`: internal iterator variables and doc comments updated (`story` → `task`, "story" → "task")
- [ ] `src/ralph/store.rs`: starter `prd.json` template changed from `"userStories": []` to `"tasks": []`
- [ ] `src/app.rs`: call `p.next_task()` instead of `p.next_story()`; update local variable `story` → `task` and field accesses (`s.id`, `s.title`) accordingly
- [ ] `src/ui.rs`: update `plan.prd.user_stories.iter()` to `plan.prd.tasks.iter()`; update closure variable from `story` to `task`; update all field accesses inside the closure (`story.passes` → `task.passes`, etc.)
- [ ] `src/ui.rs`: panel title `"Stories"` → `"Tasks"` (the static case and the formatted `"Stories ({}/{})"` case)
- [ ] `src/ui.rs`: continue prompt text `"Story done. Continue? [Y/n]"` → `"Task done. Continue? [Y/n]"`
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### T-002: Rename `Plan` struct → `Workflow`; rename source file `plan.rs` → `workflow.rs`

**Description:** As a developer, I need the `Plan` struct and its source file renamed so the module reflects the new "Workflow" terminology consistently.

**Acceptance Criteria:**

- [ ] `src/ralph/plan.rs` renamed to `src/ralph/workflow.rs`
- [ ] `src/ralph/mod.rs`: `pub mod plan;` changed to `pub mod workflow;`; any `pub use plan::*` or re-exports updated accordingly
- [ ] `src/ralph/workflow.rs`: struct `Plan` renamed to `Workflow`; `impl Plan` block renamed to `impl Workflow`; constructor returns `Workflow { prd }`; all doc comments updated
- [ ] `src/app.rs`: import updated from `use crate::ralph::plan::Plan;` to `use crate::ralph::workflow::Workflow;`
- [ ] `src/app.rs`: field `pub current_plan: Option<Plan>` renamed to `pub current_workflow: Option<Workflow>`
- [ ] `src/app.rs`: field initialiser `current_plan: None` updated to `current_workflow: None`
- [ ] `src/app.rs`: method `load_current_plan()` renamed to `load_current_workflow()`; all three call sites updated (`app.load_current_workflow()`, `self.load_current_workflow()`)
- [ ] `src/app.rs`: all accesses `self.current_plan` renamed to `self.current_workflow`; local variable `plan` bindings renamed to `workflow` where they refer to the loaded `Workflow`
- [ ] `src/ui.rs`: `&app.current_plan` → `&app.current_workflow`; pattern variables updated (`Some(plan)` → `Some(workflow)`)
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### T-003: Rename `Store` methods and change on-disk path to `.ralph/workflows/`

**Description:** As a developer, I need the `Store` methods and the on-disk path updated from `plans` to `workflows` so the new naming is consistent from the API down to the filesystem.

**Acceptance Criteria:**

- [ ] `src/ralph/store.rs`: `fn plans_dir()` renamed to `fn workflows_dir()`; path `.ralph/plans` changed to `.ralph/workflows`
- [ ] `src/ralph/store.rs`: `fn plan_dir()` renamed to `fn workflow_dir()`
- [ ] `src/ralph/store.rs`: `fn list_plans()` renamed to `fn list_workflows()`; internal variable `plans` renamed to `workflows`
- [ ] `src/ralph/store.rs`: `fn create_plan()` renamed to `fn create_workflow()`; error message `"plan '{}' already exists"` changed to `"workflow '{}' already exists"`; `create_dir_all` error message updated; all internal calls to `workflow_dir()` and `workflows_dir()` updated
- [ ] `src/ralph/store.rs`: all doc comments updated to say "workflow(s)" instead of "plan(s)"
- [ ] `src/app.rs`: `pub plans: Vec<String>` → `pub workflows: Vec<String>`
- [ ] `src/app.rs`: `pub selected_plan: Option<usize>` → `pub selected_workflow: Option<usize>`
- [ ] `src/app.rs`: `App::new()` — initialiser fields and call to `store.list_workflows()` updated
- [ ] `src/app.rs`: `fn refresh_plans_after_delete()` → `fn refresh_workflows_after_delete()`; all three internals updated (`list_workflows`, field names)
- [ ] `src/app.rs`: `fn refresh_plans_and_focus()` → `fn refresh_workflows_and_focus()`; internals updated
- [ ] `src/app.rs`: all `self.store.plan_dir()` calls → `self.store.workflow_dir()`
- [ ] `src/app.rs`: all `self.store.list_plans()` calls → `self.store.list_workflows()`
- [ ] `src/app.rs`: all `self.store.create_plan()` calls → `self.store.create_workflow()`; error message check `"already exists"` still matches the new store error string
- [ ] `src/app.rs`: all `self.plans` field accesses → `self.workflows`; all `self.selected_plan` → `self.selected_workflow`
- [ ] `src/app.rs`: all call sites for the renamed helper methods updated (`refresh_workflows_after_delete`, `refresh_workflows_and_focus`)
- [ ] `src/ui.rs`: `app.plans` → `app.workflows`; `app.plans.len()` → `app.workflows.len()`; `app.plans.iter()` → `app.workflows.iter()`; `app.plans.is_empty()` → `app.workflows.is_empty()`; `app.selected_plan` → `app.selected_workflow`
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### T-004: Rename `Dialog` variants `NewPlan`/`DeletePlan` → `NewWorkflow`/`DeleteWorkflow` and update all remaining UI labels

**Description:** As a developer, I need the `Dialog` enum variants and all remaining user-visible strings updated so the UI consistently says "Workflow" instead of "Plan" and the Rust dialog arms match.

**Acceptance Criteria:**

- [ ] `src/app.rs`: `Dialog::NewPlan { input, error }` → `Dialog::NewWorkflow { input, error }`
- [ ] `src/app.rs`: `Dialog::DeletePlan { name }` → `Dialog::DeleteWorkflow { name }`
- [ ] `src/app.rs`: `fn open_new_plan_dialog()` → `fn open_new_workflow_dialog()`; keybinding call site updated
- [ ] `src/app.rs`: `fn open_delete_plan_dialog()` → `fn open_delete_workflow_dialog()`; keybinding call site updated
- [ ] `src/app.rs`: all match arms on `Dialog::NewPlan`/`Dialog::DeletePlan` updated to the new variant names in `handle_dialog_key()` and elsewhere
- [ ] `src/app.rs`: error string `"Plan already exists"` → `"Workflow already exists"`
- [ ] `src/app.rs`: comment `// DeletePlan confirmation` → `// DeleteWorkflow confirmation`
- [ ] `src/app.rs`: claude command arg `"Implement the next user story."` → `"Implement the next task."`
- [ ] `src/ui.rs`: `Dialog::NewPlan` → `Dialog::NewWorkflow`; `Dialog::DeletePlan` → `Dialog::DeleteWorkflow`
- [ ] `src/ui.rs`: function `draw_new_plan_dialog` renamed to `draw_new_workflow_dialog`; call site updated
- [ ] `src/ui.rs`: function `draw_delete_plan_dialog` renamed to `draw_delete_workflow_dialog`; call site updated
- [ ] `src/ui.rs`: `draw_delete_workflow_dialog` — text `"Delete plan '{name}'? [y/N]"` → `"Delete workflow '{name}'? [y/N]"`; block title `"Delete Plan"` → `"Delete Workflow"`
- [ ] `src/ui.rs`: `draw_new_workflow_dialog` — prompt `"New plan name: {}_"` → `"New workflow name: {}_"`; block title `"New Plan"` → `"New Workflow"`
- [ ] `src/ui.rs`: empty-state message `"No plans. Press [n] to create one."` → `"No workflows. Press [n] to create one."`
- [ ] `src/ui.rs`: `"Select a plan"` → `"Select a workflow"`
- [ ] `src/ui.rs`: help text lines — `"new plan"` → `"new workflow"` and `"delete plan"` → `"delete workflow"`
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

### T-005: Migrate on-disk data — move `.ralph/plans/` to `.ralph/workflows/` and update JSON files

**Description:** As a developer, I need the existing on-disk data migrated so the app can find its workflows under the new path and the JSON files use the new `"tasks"` key.

**Note:** This task must be done after T-003 (the code now resolves `.ralph/workflows/`). Until this task is complete, the running app will show an empty workflow list.

**Acceptance Criteria:**

- [ ] `.ralph/plans/` directory moved to `.ralph/workflows/` (all subdirectories and files preserved)
- [ ] All 5 existing `prd.json` files have their `"userStories"` key renamed to `"tasks"` (the key name only; all array contents remain intact)
  - `.ralph/workflows/ralph-cli-core/prd.json`
  - `.ralph/workflows/ralph-cli-prd-editor/prd.json`
  - `.ralph/workflows/ralph-cli-prd-synthesis/prd.json`
  - `.ralph/workflows/ralph-cli-token-tracking/prd.json`
  - `.ralph/workflows/ralph-cli-file-watcher/prd.json`
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` passes

---

## Functional Requirements

- FR-1: The JSON field for the list of tasks in `prd.json` must be `"tasks"` (was `"userStories"`)
- FR-2: The on-disk directory for workflow data must be `<repo_root>/.ralph/workflows/` (was `.ralph/plans/`)
- FR-3: All Rust structs, field names, method names, and enum variants must use `Task`/`Workflow` terminology
- FR-4: All TUI panel titles, dialog titles, prompts, and help text must use "Task"/"Workflow" terminology
- FR-5: The claude subprocess command argument must say "Implement the next task." (was "user story")
- FR-6: `cargo build` and `cargo clippy -- -D warnings` must pass after each individual task

## Non-Goals

- No rename of `PrdJson` — it describes the file format, not the domain concept
- No rename of `prd.json` filename — the file on disk stays `prd.json`
- No rename of the `Store` struct itself
- No rename of acceptance-criteria field names (`acceptanceCriteria`, `passes`, `priority`)
- No changes to the `branchName`, `description`, `validationCommands`, or `project` fields in the JSON schema
- No backward-compatibility shim for the old `"userStories"` key or `.ralph/plans/` path

## Technical Considerations

- Tasks T-001 through T-004 are strictly sequential — each must compile before the next begins
- T-005 is a pure filesystem + text-replacement operation; it does not change any `.rs` source files
- After T-003 and before T-005 the running app will show an empty workflow list — this is expected transient state
- The `PrdJson` struct's `tasks` field uses `#[serde(rename = "tasks")]` — no camelCase needed since `"tasks"` is already lowercase
- There are exactly 5 `prd.json` files to migrate, all under `.ralph/plans/`

## Success Metrics

- Zero occurrences of `UserStory`, `user_stories`, `userStories`, `next_story`, `list_plans`, `plan_dir`, `plans_dir`, `create_plan`, `NewPlan`, `DeletePlan`, `current_plan`, `selected_plan`, `load_current_plan` in the source tree after all tasks are complete
- The TUI shows "Workflows" and "Tasks" panels with no "Plan" or "Story" labels visible
- All 5 migrated `prd.json` files parse successfully with the new schema

## Open Questions

- None — scope is fully defined.
