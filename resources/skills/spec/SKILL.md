---
name: spec
description: "Generate a draft spec for a new feature with research markers. Use when planning a feature, starting a new project, or when asked to create a spec. Triggers on: create a spec, write spec for, plan this feature, requirements for, spec out."
user-invocable: true
---

# Spec Generator

Create draft specs that are clear, actionable, and include research markers for the spec-researcher agent to fill in.

---

## The Job

1. Receive a feature description from the user
2. Ask as many essential clarifying questions (with lettered options) as needed
3. Generate a structured **draft** spec based on answers, with research markers
4. Save to `.ralph/specs/[feature-name]/spec-source.md`

**Important:** Do NOT start implementing. Just create the draft spec. The spec-researcher agent will enrich it with research findings before finalization.

---

## Step 1: Clarifying Questions

Ask only critical questions where the initial prompt is ambiguous. Focus on:

- **Problem/Goal:** What problem does this solve?
- **Core Functionality:** What are the key actions?
- **Scope/Boundaries:** What should it NOT do?
- **Success Criteria:** How do we know it's done?

### Format Questions Like This

```
1. What is the primary goal of this feature?
   A. Improve user onboarding experience
   B. Increase user retention
   C. Reduce support burden
   D. Other: [please specify]

2. Who is the target user?
   A. New users only
   B. Existing users only
   C. All users
   D. Admin users only

3. What is the scope?
   A. Minimal viable version
   B. Full-featured implementation
   C. Just the backend/API
   D. Just the UI
```

This lets users respond with "1A, 2C, 3B" for quick iteration. Remember to indent the options.

---

## Step 2: Spec Structure

Generate the spec with these sections. This is a **draft** spec — sections 3 and 4 are preliminary and will be refined after research.

### 1. Introduction/Overview

Brief description of the feature and the problem it solves.

### 2. Goals

Specific, measurable objectives (bullet list).

### 3. Tasks [DRAFT]

Mark the section heading as `## Tasks [DRAFT]` in the output.

Each task needs:

- **Title:** Short descriptive name
- **Description:** "As a [user], I want [feature] so that [benefit]"
- **Acceptance Criteria:** Verifiable checklist of what "done" means

Each task should be small enough to implement in one focused session.

**Format:**

```markdown
### TASK-001: [Title]

**Description:** As a [user], I want [feature] so that [benefit].

**Acceptance Criteria:**

- [ ] Specific verifiable criterion
- [ ] Another criterion
- [ ] Typecheck/lint passes
- [ ] **[UI tasks only]** Verify in browser using dev-browser skill
```

**Important:**

- Acceptance criteria must be verifiable, not vague. "Works correctly" is bad. "Button shows confirmation dialog before deleting" is good.
- **For any task with UI changes:** Always include "Verify in browser using dev-browser skill" as acceptance criteria. This ensures visual verification of frontend work.

### 4. Functional Requirements [DRAFT]

Mark the section heading as `## Functional Requirements [DRAFT]` in the output.

Numbered list of specific functionalities:

- "FR-1: The system must allow users to..."
- "FR-2: When a user clicks X, the system must..."

Be explicit and unambiguous.

### 5. Non-Goals (Out of Scope)

What this feature will NOT include. Critical for managing scope.

### 6. Design Considerations (Optional)

- UI/UX requirements
- Link to mockups if available
- Relevant existing components to reuse

### 7. Technical Considerations (Optional)

- Known constraints or dependencies
- Integration points with existing systems
- Performance requirements

### 8. Success Metrics

How will success be measured?

- "Reduce time to complete X by 50%"
- "Increase conversion rate by 10%"

### 9. Open Questions

Remaining questions or areas needing clarification.

### 10. Research Needed

List specific topics the spec-researcher agent should investigate. Include:

- Best practices and patterns for the problem domain
- Libraries or dependencies that might be relevant
- How competitors or similar projects handle this
- Existing codebase patterns or modules that relate to this feature

**Format in the output spec:**

```markdown
## Research Needed

- [ ] Best practices for [specific topic]
- [ ] Evaluate libraries/dependencies for [specific need]
- [ ] How do [competitors/similar tools] handle [feature]?
- [ ] Analyze existing codebase: [specific modules/patterns to examine]
- [ ] [Any other research topics relevant to this feature]
```

Each item should be specific enough that a researcher agent knows exactly what to look for.

### 11. Research Findings

Add an empty section with the following subsections. The spec-researcher agent will fill these in.

**Format in the output spec:**

```markdown
## Research Findings

### Best Practices

_To be filled by spec-researcher agent._

### Library/Dependency Analysis

_To be filled by spec-researcher agent._

### Competitive Analysis

_To be filled by spec-researcher agent._

### Codebase Analysis

_To be filled by spec-researcher agent._
```

---

## Writing for Junior Developers

The spec reader may be a junior developer or AI agent. Therefore:

- Be explicit and unambiguous
- Avoid jargon or explain it
- Provide enough detail to understand purpose and core logic
- Number requirements for easy reference
- Use concrete examples where helpful

---

## Output

- **Format:** Markdown (`.md`)
- **Location:** `.ralph/specs/[feature-name]/`
- **Filename:** `spec-source.md`

---

## Example Spec

```markdown
# Spec: Task Priority System

## Introduction

Add priority levels to tasks so users can focus on what matters most. Tasks can be marked as high, medium, or low priority, with visual indicators and filtering to help users manage their workload effectively.

## Goals

- Allow assigning priority (high/medium/low) to any task
- Provide clear visual differentiation between priority levels
- Enable filtering and sorting by priority
- Default new tasks to medium priority

## Tasks [DRAFT]

### TASK-001: Add priority field to database

**Description:** As a developer, I need to store task priority so it persists across sessions.

**Acceptance Criteria:**

- [ ] Add priority column to tasks table: 'high' | 'medium' | 'low' (default 'medium')
- [ ] Generate and run migration successfully
- [ ] Typecheck passes

### TASK-002: Display priority indicator on task cards

**Description:** As a user, I want to see task priority at a glance so I know what needs attention first.

**Acceptance Criteria:**

- [ ] Each task card shows colored priority badge (red=high, yellow=medium, gray=low)
- [ ] Priority visible without hovering or clicking
- [ ] Typecheck passes
- [ ] Verify in browser using dev-browser skill

### TASK-003: Add priority selector to task edit

**Description:** As a user, I want to change a task's priority when editing it.

**Acceptance Criteria:**

- [ ] Priority dropdown in task edit modal
- [ ] Shows current priority as selected
- [ ] Saves immediately on selection change
- [ ] Typecheck passes
- [ ] Verify in browser using dev-browser skill

### TASK-004: Filter tasks by priority

**Description:** As a user, I want to filter the task list to see only high-priority items when I'm focused.

**Acceptance Criteria:**

- [ ] Filter dropdown with options: All | High | Medium | Low
- [ ] Filter persists in URL params
- [ ] Empty state message when no tasks match filter
- [ ] Typecheck passes
- [ ] Verify in browser using dev-browser skill

## Functional Requirements [DRAFT]

- FR-1: Add `priority` field to tasks table ('high' | 'medium' | 'low', default 'medium')
- FR-2: Display colored priority badge on each task card
- FR-3: Include priority selector in task edit modal
- FR-4: Add priority filter dropdown to task list header
- FR-5: Sort by priority within each status column (high to medium to low)

## Non-Goals

- No priority-based notifications or reminders
- No automatic priority assignment based on due date
- No priority inheritance for subtasks

## Technical Considerations

- Reuse existing badge component with color variants
- Filter state managed via URL search params
- Priority stored in database, not computed

## Success Metrics

- Users can change priority in under 2 clicks
- High-priority tasks immediately visible at top of lists
- No regression in task list performance

## Open Questions

- Should priority affect task ordering within a column?
- Should we add keyboard shortcuts for priority changes?

## Research Needed

- [ ] Best practices for priority systems in task management UIs
- [ ] Evaluate UI component libraries for priority badge/indicator patterns
- [ ] How do Todoist, Linear, and Asana handle task priority?
- [ ] Analyze existing codebase: task card component, list filtering, database schema

## Research Findings

### Best Practices

_To be filled by spec-researcher agent._

### Library/Dependency Analysis

_To be filled by spec-researcher agent._

### Competitive Analysis

_To be filled by spec-researcher agent._

### Codebase Analysis

_To be filled by spec-researcher agent._
```

---

## Checklist

Before saving the spec:

- [ ] Asked clarifying questions with lettered options
- [ ] Incorporated user's answers
- [ ] Tasks are small and specific
- [ ] Tasks section heading includes `[DRAFT]` marker
- [ ] Functional requirements are numbered and unambiguous
- [ ] Functional Requirements section heading includes `[DRAFT]` marker
- [ ] Non-goals section defines clear boundaries
- [ ] `## Research Needed` section lists specific topics for the researcher agent
- [ ] `## Research Findings` section present with empty subsections (Best Practices, Library/Dependency Analysis, Competitive Analysis, Codebase Analysis)
- [ ] Saved to `.ralph/specs/[feature-name]/spec-source.md`
