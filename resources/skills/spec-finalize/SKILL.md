---
name: spec-finalize
description: "Consolidate research findings and finalize a draft spec. Aggregates any research files, resolves open questions, and applies accepted refinements. Use after the spec-researcher agent has run (research is optional). Triggers on: finalize spec, finalize the spec, spec-finalize."
user-invocable: true
---

# Spec Finalize

Consolidate research findings from the `researches/` directory, resolve open questions, apply accepted refinements, and produce a finalized spec.

---

## The Job

1. Read project context (CLAUDE.md and referenced docs)
2. Read the draft spec from `.ralph/specs/<feature>/spec-source.md`
3. Aggregate any research files from `.ralph/specs/<feature>/researches/` into a unified summary
4. Write the consolidated findings into `## Research Findings` in spec-source.md
5. Summarize key research findings for the user
6. Present open questions as lettered-option choices
7. Present suggested refinements and ask the user to accept, reject, or modify each
8. Update the spec in-place: resolve questions, apply refinements, remove draft markers, clean up temporary sections
9. Save the finalized spec

---

## Step 0: Read Project Context

Read `CLAUDE.md` in the repo root. If it references other documents (e.g. `COMPONENT_ARCHITECTURE.md`, `README.md`), read those too. Apply any conventions, patterns, or constraints described in these documents throughout your work.

---

## Step 1: Read the Draft Spec

Read the spec path from `$SPEC_FILE`. If the variable is set, use that path. Otherwise ask the user which spec to finalize. The file is at `.ralph/specs/<feature>/spec-source.md`.

Read the entire file.

---

## Step 2: Aggregate Research Files

Check for research files in `.ralph/specs/<feature>/researches/`. This directory may be absent or empty — that is fine; research is optional.

If one or more research files exist:
- Read all files in the directory
- Group findings by subsection: Best Practices, Library/Dependency Analysis, Competitive Analysis, Codebase Analysis
- Merge findings from all files; de-duplicate intelligently (do not repeat identical findings; merge near-duplicates into a single clear statement)
- Produce a single unified summary across all research files — do NOT present findings per-file

Write the consolidated findings into `## Research Findings` in spec-source.md, replacing any existing placeholder text. The subsections are:

```markdown
## Research Findings

### Best Practices

- **[Topic]**: [Finding].

### Library/Dependency Analysis

- **[Topic]**: [Finding].

### Competitive Analysis

- **[Topic]**: [Finding].

### Codebase Analysis

- **[Topic]**: [Finding]. (See: `path/to/file.rs`)
```

If no research files exist, skip this step entirely — do not warn, do not stop, proceed to Step 3.

---

## Step 3: Summarize Research Findings

If research files were found and consolidated, present a concise summary to the user:

```
Here are the key findings from research:

**Best Practices:**
- [1-2 sentence summary of each major finding]

**Library/Dependency Analysis:**
- [1-2 sentence summary of each major finding]

**Competitive Analysis:**
- [1-2 sentence summary of each major finding]

**Codebase Analysis:**
- [1-2 sentence summary of each major finding]
```

Keep summaries brief. The user can read the full spec for details. The goal is to give enough context to make informed decisions in the next steps.

If no research was found, skip this step and proceed directly to Step 4.

---

## Step 4: Present Open Questions

Read the `## Open Questions from Research` section if it exists, and any open questions present elsewhere in the spec. Present each question as a lettered-option choice.

### Format

```
The following open questions need resolution:

1. [Question text]? (Context: [why it matters])
   A. [Option based on research finding or common approach]
   B. [Alternative option]
   C. [Another alternative if applicable]
   D. Other: [please specify]

2. [Next question]?
   A. [Option]
   B. [Option]
   C. Other: [please specify]
```

Derive the options from the research findings when possible. Always include an "Other" option.

Let the user respond with shorthand like "1A, 2B" for efficiency.

If there are no open questions, skip to Step 5.

---

## Step 5: Present Suggested Refinements

Read the `## Suggested Refinements` section if it exists. Present each refinement individually and ask the user to accept, reject, or modify it.

### Format

```
The following refinements are suggested:

1. **[Area]**: [Specific suggestion and rationale]
   -> Accept / Reject / Modify?

2. **[Area]**: [Specific suggestion and rationale]
   -> Accept / Reject / Modify?
```

The user can respond with shorthand like "1 accept, 2 reject, 3 modify: change X to Y".

Collect all decisions before proceeding to the update step.

If there are no suggested refinements, skip to Step 6.

---

## Step 6: Update the Spec In-Place

Apply all user decisions to the spec file. Perform these changes:

### 6a. Resolve Open Questions

For each answered question, integrate the decision into the appropriate spec section:
- If the answer affects a task, update that task's description or acceptance criteria
- If the answer affects functional requirements, update the relevant FR items
- If the answer affects technical considerations, update that section
- If the answer belongs in a new or existing section, place it there

### 6b. Apply Accepted Refinements

For each accepted refinement:
- Make the specific change described in the refinement
- If the refinement references a task or requirement by ID, update that item directly

For modified refinements, apply the user's modified version instead.

Rejected refinements require no changes.

### 6c. Remove Draft Markers

Remove `[DRAFT]` from all section headings:
- `## Tasks [DRAFT]` becomes `## Tasks`
- `## Functional Requirements [DRAFT]` becomes `## Functional Requirements`

### 6d. Remove Temporary Sections

Remove these sections entirely from the spec:
- `## Research Needed` (the research is done)
- `## Suggested Refinements` (decisions have been applied)
- `## Open Questions from Research` (questions have been resolved)

### 6e. Retain Research Findings

Keep the `## Research Findings` section and its subsections in the final spec. This serves as a reference for implementation agents.

### 6f. Save

Write the updated spec back to the same file (`.ralph/specs/<feature>/spec-source.md`).

---

## Checklist Before Saving

Before writing the finalized spec:

- [ ] All open questions have been resolved (user answered each one)
- [ ] All suggested refinements have been addressed (accepted, rejected, or modified)
- [ ] `[DRAFT]` markers removed from all section headings
- [ ] `## Research Needed` section removed
- [ ] `## Suggested Refinements` section removed
- [ ] `## Open Questions from Research` section removed
- [ ] `## Research Findings` section retained (if research was performed)
- [ ] Task acceptance criteria still include "Typecheck passes"
- [ ] File saved to same path (`.ralph/specs/<feature>/spec-source.md`)

---

## Rules

- Always present findings and questions before making changes -- the user decides
- Never skip a question or refinement without user input
- Do not add new sections or content beyond what the user approves
- Do not modify `## Research Findings` content after it has been written and reviewed
- Keep the existing spec structure and section ordering intact (except for removed sections)
- If the user wants to skip the review process, warn that open questions will remain unresolved, then proceed to just remove draft markers and temporary sections
