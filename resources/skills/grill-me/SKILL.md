---
name: grill-me
description: "Interview the user relentlessly about a plan, spec, or design until reaching shared understanding. Can spawn the researcher agent to investigate questions that need data. Use when user wants to stress-test a plan, get grilled on their design, challenge assumptions, or mentions grill me."
user-invocable: true
---

# Grill Me

Stress-test a plan, spec, or design through relentless questioning. Resolve every branch of the decision tree before moving forward.

---

## The Job

1. Identify what to grill (spec, plan, design, or idea in context)
2. Interview the user relentlessly about every aspect
3. When a question needs data, either explore the codebase or spawn the researcher agent
4. Resolve each branch of the decision tree one-by-one
5. Produce a summary of decisions made and update the source document if applicable

---

## Step 1: Identify the Subject

Look for the subject to grill in this order:

1. **Spec file**: Check if a spec is referenced in context or at `.ralph/specs/<feature>/spec-source.md`
2. **Plan or design**: Check if a plan file or design doc is in context
3. **Conversation context**: The user may have just described an idea or plan
4. **Ask**: If nothing is obvious, ask: "What would you like me to grill you on?"

Read the full document before starting.

---

## Step 2: Interview Relentlessly

Walk down each branch of the design tree, resolving dependencies between decisions one-by-one.

### Interview Rules

- **For each question, provide your recommended answer** based on what you know about the codebase and the domain
- **If a question can be answered by exploring the codebase, explore instead of asking** -- don't waste the user's time on things you can look up
- **Resolve one branch at a time** -- don't jump between unrelated topics
- **Go deep before going wide** -- exhaust one area before moving to the next
- **Challenge assumptions** -- if the user says "we'll just do X", ask why X and not Y
- **Ask about what's missing** -- gaps in specs are where bugs hide

### What to Challenge

#### Architecture & Design
- Why this approach over alternatives?
- What are the failure modes? How do we recover?
- What happens at scale? At zero? At one?
- Where are the integration boundaries?
- Which parts are likely to change? Are they isolated?

#### Scope & Priority
- If we cut this in half, which half ships?
- What's the simplest version that delivers value?
- What's explicitly out of scope? Why?
- Are there hidden dependencies?

#### User Experience
- What happens on first use? Empty states?
- What about error states? Slow connections? Concurrent access?
- Who are the edge-case users?

#### Testing & Quality
- How do we know this works?
- What's the testing strategy for each module?
- Where are the boundaries for mocking?
- What's the prior art for tests in this codebase?

#### Implementation
- Are the modules deep enough? (small interface, big implementation)
- Can each module be tested in isolation?
- What's the migration path? Can we ship incrementally?
- What are the durable decisions (schemas, routes, models) vs implementation details?

### Question Format

Use lettered options for efficient responses:

```
1. How should we handle authentication for the new API endpoints?
   A. Extend the existing auth middleware (recommended -- consistent with current patterns)
   B. New auth layer specific to this feature
   C. Token-based auth with separate token management
   D. Other: [please specify]

   My recommendation: A, because [reason based on codebase exploration].
```

---

## Step 3: Spawn Researcher When Needed

When a question arises that needs **external data** to answer properly -- best practices, library comparisons, competitive analysis, or deeper codebase investigation -- spawn the researcher agent instead of guessing.

### When to Spawn the Researcher

- "What's the best library for X?" -- needs library comparison
- "How do others handle Y?" -- needs competitive analysis
- "What's the recommended pattern for Z?" -- needs best practices research
- "How does our codebase currently handle W?" -- needs deep codebase analysis (beyond what quick grep can answer)

### How to Spawn

Use the Agent tool to spawn the `prd-researcher` agent:

```
Use the Agent tool with subagent_type="prd-researcher" and provide a focused prompt:

"Research the following question that came up during spec review:

**Question:** [The specific question]
**Context:** [Relevant context from the spec/plan]
**What we need:** [Specific information needed to make a decision]

The spec is at: [path to spec file if applicable]

Focus only on this specific question. Report findings concisely."
```

While the researcher runs (use `run_in_background: true`), continue grilling the user on questions that don't need research. When the researcher returns, present findings and resolve the branch.

### Multiple Research Questions

If several questions need research, batch them into a single researcher spawn when they're related. Spawn separate researchers for unrelated topics (in parallel).

---

## Step 4: Resolve and Record

As decisions are made during the interview:

### Track Decisions

Keep a running list of decisions made:

```
Decisions so far:
1. Auth: Extend existing middleware (consistent with codebase)
2. Storage: Use existing PostgreSQL schema, add new table
3. API: REST endpoints, not GraphQL (matches current API style)
4. Testing: Integration tests for API layer, unit tests for business logic module
```

Present this summary periodically so the user can confirm or correct.

### Update the Source Document

If grilling a spec file (`.ralph/specs/<feature>/spec-source.md`):

- Update **Implementation Decisions** with decisions made
- Update **Testing Decisions** with testing strategy decided
- Add resolved questions to appropriate sections
- Move answered items from **Open Questions** to the relevant section
- Add new **Research Needed** items if the researcher uncovered topics worth deeper investigation

If grilling a plan or other document, suggest specific edits to the user.

---

## Step 5: Wrap Up

When all branches are resolved:

1. Present the **full decision summary**
2. Highlight any **remaining open questions** (things that couldn't be resolved now)
3. If a spec was updated, confirm the changes with the user
4. Suggest next steps:
   - If research markers were added: "Run the spec-researcher agent to investigate the new research items"
   - If the spec is ready: "Run `/spec-finalize` to finalize the spec"
   - If this was a plan/design: summarize what should be updated in the source document

---

## Rules

- Never accept "we'll figure it out later" without pushing back -- ask what specifically will be figured out and when
- Provide your recommended answer for every question -- don't just ask, guide
- Explore the codebase proactively to ground your questions in reality
- Don't ask questions you can answer yourself by reading code
- Keep the energy up -- this should feel like a productive sparring session, not an interrogation
- One branch at a time, resolved before moving on
- If the user says "that's enough" or "let's move on", respect it -- summarize decisions and wrap up
