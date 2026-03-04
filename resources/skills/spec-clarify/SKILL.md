---
name: spec-clarify
description: "Review research findings and finalize a draft spec. Presents research summaries, resolves open questions, and applies accepted refinements. Use after the spec-researcher agent has enriched a draft spec. Triggers on: clarify spec, finalize spec, review research findings, spec-clarify."
user-invocable: true
---

# Spec Clarify

Present research findings to the user, resolve open questions, apply accepted refinements, and produce a finalized spec.

---

## The Job

1. Read the enriched spec from `.ralph/specs/<feature>/spec-source.md`
2. Summarize key research findings for the user
3. Present open questions as lettered-option choices
4. Present suggested refinements and ask the user to accept, reject, or modify each
5. Update the spec in-place: resolve questions, apply refinements, remove draft markers, clean up temporary sections
6. Save the finalized spec

---

## Step 1: Read the Enriched Spec

Ask the user which spec to finalize if not obvious from context. The file is at `.ralph/specs/<feature>/spec-source.md`.

Read the entire file. Verify it contains:
- `## Research Findings` with populated subsections
- `## Suggested Refinements` (from the researcher agent)
- `## Open Questions from Research` (from the researcher agent)

If any of these sections are missing, warn the user that the spec may not have been through the research step. Ask whether to proceed anyway.

---

## Step 2: Summarize Research Findings

Present a concise summary of the research findings to the user. Group by subsection:

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

---

## Step 3: Present Open Questions

Read the `## Open Questions from Research` section. Present each question as a lettered-option choice.

### Format

```
The researcher identified these open questions:

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

---

## Step 4: Present Suggested Refinements

Read the `## Suggested Refinements` section. Present each refinement individually and ask the user to accept, reject, or modify it.

### Format

```
The researcher suggests these refinements:

1. **[Area]**: [Specific suggestion and rationale]
   -> Accept / Reject / Modify?

2. **[Area]**: [Specific suggestion and rationale]
   -> Accept / Reject / Modify?
```

The user can respond with shorthand like "1 accept, 2 reject, 3 modify: change X to Y".

Collect all decisions before proceeding to the update step.

---

## Step 5: Update the Spec In-Place

Apply all user decisions to the spec file. Perform these changes:

### 5a. Resolve Open Questions

For each answered question, integrate the decision into the appropriate spec section:
- If the answer affects a task, update that task's description or acceptance criteria
- If the answer affects functional requirements, update the relevant FR items
- If the answer affects technical considerations, update that section
- If the answer belongs in a new or existing section, place it there

### 5b. Apply Accepted Refinements

For each accepted refinement:
- Make the specific change described in the refinement
- If the refinement references a task or requirement by ID, update that item directly

For modified refinements, apply the user's modified version instead.

Rejected refinements require no changes.

### 5c. Remove Draft Markers

Remove `[DRAFT]` from all section headings:
- `## Tasks [DRAFT]` becomes `## Tasks`
- `## Functional Requirements [DRAFT]` becomes `## Functional Requirements`

### 5d. Remove Temporary Sections

Remove these sections entirely from the spec:
- `## Research Needed` (the research is done)
- `## Suggested Refinements` (decisions have been applied)
- `## Open Questions from Research` (questions have been resolved)

### 5e. Retain Research Findings

Keep the `## Research Findings` section and its subsections in the final spec. This serves as a reference for implementation agents.

### 5f. Save

Write the updated spec back to the same file (`.ralph/specs/<feature>/spec-source.md`).

---

## Checklist Before Saving

Before writing the finalized spec:

- [ ] All open questions from research have been resolved (user answered each one)
- [ ] All suggested refinements have been addressed (accepted, rejected, or modified)
- [ ] `[DRAFT]` markers removed from all section headings
- [ ] `## Research Needed` section removed
- [ ] `## Suggested Refinements` section removed
- [ ] `## Open Questions from Research` section removed
- [ ] `## Research Findings` section retained with populated subsections
- [ ] Task acceptance criteria still include "Typecheck passes"
- [ ] File saved to same path (`.ralph/specs/<feature>/spec-source.md`)

---

## Rules

- Always present findings and questions before making changes -- the user decides
- Never skip a question or refinement without user input
- Do not add new sections or content beyond what the user approves
- Do not modify `## Research Findings` content (it is a reference record)
- Keep the existing spec structure and section ordering intact (except for removed sections)
- If the user wants to skip the review process, warn that open questions will remain unresolved, then proceed to just remove draft markers and temporary sections
