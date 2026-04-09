---
name: spec
description: "Generate a draft PRD/spec for a new feature through deep user interview, codebase exploration, module design, and research markers. Use when planning a feature, starting a new project, or when asked to create a spec/PRD. Triggers on: create a spec, write spec for, plan this feature, requirements for, spec out, write a prd."
user-invocable: true
---

# Spec Generator

Create draft specs that are clear, actionable, and built on deep shared understanding through relentless interviewing, codebase exploration, and module design.

---

## The Job

1. Receive a feature description from the user
2. Ask for a long, detailed description of the problem and any solution ideas
3. Explore the codebase to verify assertions and understand the current state
4. Interview the user relentlessly until reaching shared understanding
5. Sketch major modules and identify deep module opportunities
6. Generate a structured **draft** spec with research markers
7. Save to `.ralph/specs/[feature-name]/spec-source.md`

**Important:** Do NOT start implementing. Just create the draft spec. The spec-researcher agent will enrich it with research findings before finalization.

**Tip:** After generating the spec, suggest the user run `/grill-me` to stress-test the design before moving to research and finalization.

---

## Step 1: Gather Context

Ask the user for a **long, detailed description** of:

- The problem they want to solve
- Any potential ideas for solutions
- Why this matters now
- **Jira ticket number** (optional) — if provided, it will be included in the spec and used to prefix the branch name in the generated workflow

Don't settle for a one-liner. Push for detail:

```
Before we start, I need a thorough description of the problem and any solution ideas you have.
Don't hold back -- the more detail, the better the spec. Tell me:

1. What problem are you solving? Who feels the pain?
2. What does your ideal solution look like?
3. Any constraints, deadlines, or dependencies I should know about?
4. Do you have a Jira ticket number for this? (optional -- e.g. PROJ-1234)
```

---

## Step 2: Explore the Codebase

Before interviewing, explore the repo to:

- Verify the user's assertions about the current state
- Understand existing architecture, patterns, and integration layers
- Identify relevant modules, files, and conventions
- Spot potential conflicts or complications

Use the Agent tool with `subagent_type=Explore` for broad exploration, or Glob/Grep for targeted searches.

This grounds the interview in reality -- you can ask better questions when you know the codebase.

---

## Step 3: Interview Relentlessly

Interview the user about **every aspect** of the plan until you reach shared understanding. This is the most important step.

### How to Interview

- Walk down each branch of the design tree
- Resolve dependencies between decisions one-by-one
- For each question, **provide your recommended answer** based on codebase exploration
- If a question can be answered by exploring the codebase, explore instead of asking
- Use lettered options for quick responses (user can reply "1A, 2C, 3B")

### What to Cover

- **Problem space:** Who are the users? What's the current workaround? How bad is the pain?
- **Solution boundaries:** What should it do? What should it NOT do? Where does scope end?
- **Edge cases:** What happens when X fails? What about empty states? Concurrent access?
- **Integration:** How does this connect to existing features? What breaks if we get it wrong?
- **Priorities:** If we can only ship half of this, which half? What's the MVP?

### Format Questions Like This

```
1. How should we handle the case where a user has no existing data?
   A. Show an empty state with a CTA to create first item (recommended -- matches existing patterns in the codebase)
   B. Pre-populate with sample data
   C. Redirect to an onboarding flow
   D. Other: [please specify]

2. Should the API be synchronous or async?
   A. Synchronous -- simpler, fine for small payloads (recommended based on current API patterns)
   B. Async with polling -- needed if processing takes >2s
   C. Async with webhooks
   D. Other: [please specify]
```

Keep going until there are no more open branches. Don't stop after 3-4 questions -- a good interview might have 10-20 questions across multiple rounds.

---

## Step 4: Module Design

Sketch out the major modules needed. Actively look for opportunities to extract **deep modules**.

### Deep Modules

A deep module (from "A Philosophy of Software Design") encapsulates a lot of functionality behind a simple, testable interface that rarely changes.

- **Deep module** = small interface + lots of implementation (GOOD)
- **Shallow module** = large interface + little implementation (AVOID)

### What to Sketch

For each major module:

- **Name**: What is it called?
- **Responsibility**: What does it own?
- **Interface**: What's the public API? (keep it small)
- **Dependencies**: What does it need from other modules?
- **Testability**: Can it be tested in isolation?

### Check with the User

Present the module sketch and ask:

```
Here are the major modules I see:

1. **[Module A]**: [responsibility]. Interface: [brief]. Testable in isolation: yes/no.
2. **[Module B]**: [responsibility]. Interface: [brief]. Testable in isolation: yes/no.

Questions:
- Do these match your mental model?
- Which modules do you want tests written for?
- Any modules I'm missing?
```

---

## Step 5: Write the Spec

Generate the spec with these sections. Sections 3 and 4 are preliminary and will be refined after research.

### 1. Introduction/Overview

Brief description of the feature and the problem it solves.

If a Jira ticket was provided, include it as metadata at the top of the spec:

```markdown
**Jira Ticket:** PROJ-1234
```

If no Jira ticket was provided, omit this line entirely.

### 2. Goals

Specific, measurable objectives (bullet list).

### 3. User Stories [DRAFT]

Mark the section heading as `## User Stories [DRAFT]` in the output.

A **long, numbered list** of user stories covering ALL aspects of the feature. Each story in the format:

```
1. As a [actor], I want [feature], so that [benefit]
```

This list should be extremely extensive. Think about:
- Primary workflows
- Edge cases and error states
- Admin/power user scenarios
- First-time vs returning user experiences
- Accessibility needs

### 4. Tasks [DRAFT]

Mark the section heading as `## Tasks [DRAFT]` in the output.

Break user stories into implementable tasks. Each task needs:

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
- **For any task with UI changes:** Always include "Verify in browser using dev-browser skill" as acceptance criteria.

### 5. Implementation Decisions

Document the decisions made during the interview:

- **Modules**: The modules that will be built/modified (from Step 4)
- **Module interfaces**: The public APIs of those modules
- **Architectural decisions**: Patterns, approaches, trade-offs chosen
- **Schema changes**: Database or data model changes
- **API contracts**: Endpoint shapes, request/response formats
- **Technical clarifications**: Answers to technical questions from the interview

Do NOT include specific file paths or code snippets -- they outdated quickly.

### 6. Testing Decisions

Document testing strategy:

- **What makes a good test**: Only test external behavior through public interfaces, not implementation details
- **Which modules will be tested**: Reference the modules from Step 4
- **Prior art**: Similar types of tests already in the codebase
- **Test boundaries**: Where to mock (system boundaries only -- external APIs, databases, time/randomness)

### 7. Non-Goals (Out of Scope)

What this feature will NOT include. Critical for managing scope.

### 8. Design Considerations (Optional)

- UI/UX requirements
- Link to mockups if available
- Relevant existing components to reuse

### 9. Technical Considerations (Optional)

- Known constraints or dependencies
- Integration points with existing systems
- Performance requirements

### 10. Success Metrics

How will success be measured?

### 11. Open Questions

Remaining questions or areas needing clarification.

### 12. Research Needed

List specific topics the spec-researcher agent should investigate:

```markdown
## Research Needed

- [ ] Best practices for [specific topic]
- [ ] Evaluate libraries/dependencies for [specific need]
- [ ] How do [competitors/similar tools] handle [feature]?
- [ ] Analyze existing codebase: [specific modules/patterns to examine]
- [ ] [Any other research topics relevant to this feature]
```

Each item should be specific enough that a researcher agent knows exactly what to look for.

### 13. Research Findings

Add an empty section with subsections for the spec-researcher agent to fill:

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

## Writing Guidelines

The spec reader may be a junior developer or AI agent. Therefore:

- Be explicit and unambiguous
- Avoid jargon or explain it
- Provide enough detail to understand purpose and core logic
- Number requirements for easy reference
- Use concrete examples where helpful
- Do NOT include specific file paths or code snippets (they outdate quickly)

---

## Output

- **Format:** Markdown (`.md`)
- **Location:** `.ralph/specs/[feature-name]/`
- **Filename:** `spec-source.md`

---

## Checklist

Before saving the spec:

- [ ] Gathered detailed problem description and solution ideas
- [ ] Explored the codebase to verify assertions
- [ ] Interviewed the user relentlessly (multiple rounds of questions)
- [ ] Sketched modules and identified deep module opportunities
- [ ] Checked module design with the user
- [ ] User stories are extensive and cover all aspects
- [ ] User Stories section heading includes `[DRAFT]` marker
- [ ] Tasks are small and specific with verifiable acceptance criteria
- [ ] Tasks section heading includes `[DRAFT]` marker
- [ ] Implementation decisions documented (modules, interfaces, schemas, API contracts)
- [ ] Testing decisions documented (what to test, how, prior art)
- [ ] Non-goals section defines clear boundaries
- [ ] `## Research Needed` section lists specific topics for the researcher agent
- [ ] `## Research Findings` section present with empty subsections
- [ ] Saved to `.ralph/specs/[feature-name]/spec-source.md`
- [ ] Suggested user run `/grill-me` to stress-test the design
