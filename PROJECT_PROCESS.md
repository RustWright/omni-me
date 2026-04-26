# Project Development Process

This document describes the structured process for developing projects in this learning repository, with heavy AI collaboration throughout.

## Overview

Projects follow a phased approach that balances velocity, learning, and quality. The process emphasizes:
- **Early validation** through MVP development
- **Iterative refinement** based on code review feedback
- **Decision transparency** with documented rationales
- **Stabilization after velocity** through dedicated code review cycles

## Session Semantics

"Session N" denotes **a phase of work with a defined purpose**, not a single uninterrupted sitting. A single session may span multiple Claude Code invocations — context limits, token limits, or user fatigue can all force a break mid-session. The numbers exist only to indicate **ordering**, not single-sitting execution.

## Process Flow

```
First Cycle (full process):
Session 1: Initiation
    ↓
Session 2: Research
    ↓
Session 3: Architecture
    ↓
Session 4: Planning ←─────┐
    ↓                     │
Session 5: Implementation │
    ↓                     │
Session 6: Code Review ───┘

Subsequent Cycles:
Session 4 → Session 5 → Session 6  (repeat)
```

**First iteration:** Complete Sessions 1→2→3→4→5→6 to build and validate MVP.
**Subsequent iterations:** Cycle through 4→5→6 to add features and refinements. Planning may include lightweight embedded research when the scope demands it.

## AI's Role

AI (Claude Code) serves as collaborative partner throughout all sessions:

### Sessions 1-4 (Initiation, Research, Architecture, Planning)
- **Interviewing/Facilitating:** Ask structured questions to capture requirements and organize thinking. Ask questions **one at a time** to allow for clear, detailed answers.
- **Collaborating/Proposing:** Suggest architectural patterns, identify risks, propose solutions
- **Decision Support:**
  - Present multiple options with trade-offs
  - Explain alternatives in depth as needed
  - User asks clarifying questions until understanding is sufficient
  - User makes final decision (AI documents it)
  - End-of-session risk review in Session 3 (architecture) for structural concerns that could doom implementation

### Session 5 (Implementation)
- **Claude Code:** Orchestration, progress tracking, velocity with quality
- Parallelize work via worktree subagents where file boundaries allow
- Make frequent commits with clear messages
- No pausing for explanations — those belong in Code Review

### Session 6 (Code Review)
- Run multi-perspective parallel review (security, logical consistency, performance, bloat/complexity)
- Explore test-coverage gaps — what could silently break if an LLM edit regressed it?
- Drive the fix cycle: agent-assisted corrections batched into coherent commits

## Session Details

### Session 1: Project Initiation

**Purpose:** Define project foundation and motivation

**Questions to Answer:**
1. What is the goal of the project?
2. Who will be the primary user/consumer (who benefits)?
3. What does success look like?
4. What are the time/urgency expectations?
5. Why does this project matter to you right now? What's driving this need?

**Output:**
- Documented answers in `project.md`
- Clear understanding of project importance (reference point for future motivation)

---

### Session 2: Research *(first cycle only)*

**Purpose:** Deep foundational research before architectural commitments

**Activities:**
- Survey all candidate features and their service options
- Compare tech stack choices with real trade-offs (performance, maintainability, ecosystem, cost)
- Identify proof-of-concept needs (assumptions that must be validated before architecture is final)
- Document findings so architecture decisions are grounded in evidence, not defaults

**Output:**
- `research.md` with findings, service comparisons, and tentative technical direction
- Summary in `project.md` linking to `research.md`

**Note:** Research is formalized as its own session in the first cycle only. In later cycles, lightweight research can be embedded in Planning (Session 4) when a complex feature's scope demands it — skip entirely for routine cycles.

---

### Session 3: Project Architecture *(first cycle only)*

**Purpose:** Design the technical foundation and MVP scope

**Activities:**
- Define long-term architectural vision
  - File/directory structure
  - Documentation approach
  - Testing strategy
  - Tools, APIs, libraries, technologies
- Define MVP scope (minimum to validate core idea)
- Plan transition from MVP to full version
- Make architectural decisions with trade-off discussions
- End-of-session risk review

**Decision-Making Process:**
- AI presents architectural options (grounded in `research.md`)
- Discuss trade-offs, alternatives, implications
- User asks clarifying questions as needed
- User makes final decisions
- Decisions documented with brief rationale

**Output:**
- `architecture.md` created with chosen architecture and brief rationale
- Decision summaries in `project.md` (alternatives, trade-offs, deciding factors)

---

### Session 4: Project Planning *(every cycle)*

**Purpose:** Break down the next iteration into actionable tasks

**Activities:**
- Review Code Review feedback from previous cycle (if applicable)
- Decide: address immediate improvements OR continue roadmap
- **Optional embedded research:** when the cycle touches unfamiliar features/libraries/services, run a scoped research pass before committing to tasks
- Define high-level phases for current scope
- Break work into atomic chunks (≤10 lines of code where possible)
- Identify opportunities for parallel development (worktree-friendly file boundaries)
- Create `tasks.md` with detailed task list

**Output:**
- `tasks.md` created with tasks organized by status (pending/in-progress/completed)
- Planning summary in `project.md` (objective, scope, phases, any embedded research findings)

**Note:** Task list defines "done" for this iteration cycle

---

### Session 5: Implementation *(every cycle)*

**Purpose:** Build the planned functionality with velocity

**Approach:**
- Default to moving quickly while maintaining code quality
- Parallelize across worktree subagents where file boundaries allow
- Make frequent commits with clear, concise messages
- Learning pace adjustable based on project priority and user request
- No pausing for explanations — those belong in Code Review

**Output:**
- Implemented code committed to repository
- `tasks.md` updated as work progresses
- Brief summary in `project.md` (dates, actual vs planned work, key commits)

---

### Session 6: Code Review *(every cycle)*

**Purpose:** Stabilize the foundation after a velocity-focused implementation. A multi-session effort spanning as many sittings as the cycle demands.

The session is structured as **produce-all-reviews-first, then triage**. Phase A and Phase B both *generate documents*; Phase C is the only place that writes code, and it is interleaved with triage (one document at a time, one finding at a time, decide and resolve inline). There is **no separate batched "fix cycle" after triage** — fixes land as each finding is decided.

**Phase A — Multi-Perspective Review (produce all docs first):**
Four parallel review passes generate findings documents in `reviews/YYYY-MM-DD-<perspective>.md`:
- **Security** — data handling, secrets, auth gaps, input validation, injection surfaces
- **Logical consistency** — invariants, edge cases, off-by-one, error paths, contract drift between layers
- **Performance** — hot paths, unnecessary allocations, database query patterns, async misuse
- **Bloat / complexity** — dead code, over-abstraction, duplicated helpers, needless dependencies

Findings use Critical / Warning / Info priority buckets with `file:line` references. All four documents are produced up-front so the triage phase can see the full picture and spot cross-cutting issues.

**Phase B — Test Coverage Audit (proposal doc, no code yet):**
- Identify what could silently break if an LLM edit introduced a regression
- Flag untested branches in business logic, unvalidated schema assumptions, missing idempotency proofs
- Output a proposal document `reviews/YYYY-MM-DD-test-gaps.md` listing candidate tests for triage — **no test code is written in this phase**

**Phase C — Triage and Resolve (one document at a time, one item at a time):**
- Work through each review document in turn (user picks the order)
- Within a document, take each finding individually and decide: **fix now**, **defer to a later cycle**, or **accept** (with rationale)
- Annotate the disposition in the review document so it becomes the audit trail
- Fixes land inline as each finding is resolved — small, coherent commits per finding (or per small group), not one batched fix commit at the end
- Test-gap proposals are triaged item-by-item the same way: a test that locks current correct behavior may land standalone; a tripwire/regression test for a buggy finding lands together with the fix commit
- Items decided as "defer" roll forward as feedback for the next cycle's Planning session

**Output:**
- `reviews/YYYY-MM-DD-*.md` per-perspective review documents (created Phase A, annotated with disposition during Phase C)
- `reviews/YYYY-MM-DD-test-gaps.md` test-gap proposal (created Phase B, annotated during Phase C)
- Fix commits landed on the cycle's branch/main as triage progresses
- Deferred items recorded in `project.md` for the next cycle's Planning

---

## Project Documentation Structure

Each project follows this structure:

```
projects/project_name/
├── .log/                    # Raw exported conversation logs
│   ├── session-01-initiation.txt
│   ├── session-02-research.txt
│   ├── session-03-architecture.txt
│   ├── session-04-planning-cycle-1.txt
│   ├── session-05-implementation-cycle-1.txt
│   ├── session-06-code-review-cycle-1.txt
│   └── ...
├── project.md               # Persistent checklist/tracker (from template)
├── research.md              # Research findings (first cycle)
├── architecture.md          # Architecture decisions and rationale
├── tasks.md                 # Current cycle's task list (reset each cycle)
├── reviews/                 # Per-cycle code review findings
└── [project files]
```

**Information Density Hierarchy:**
1. `.log/` files = Full discussion and reasoning
2. `project.md` = Decision summaries with alternatives and trade-offs
3. `research.md` / `architecture.md` = Canonical technical decisions with brief rationale
4. `reviews/` = Code review findings per perspective per cycle

---

## Special Cases

### Architecture Revision
If a Code Review reveals fundamental architectural issues (not just features, but structure):
- Only consider if there's a very clear and obvious problem
- Trigger an emergency Session 3 to revise architecture
- Update `architecture.md` with revised decisions
- Document what changed and why in `project.md`

### Project State Changes
Update status in `project.md` metadata when:
- **Paused:** Motivation changed, need shifted to different priority
- **Completed:** Project goals achieved
- **Abandoned:** No longer pursuing

Status helps quickly identify active vs shelved projects.

---

## End-of-Session Protocol

After every session:
1. **Export conversation:** Run `/export` command to save raw log
2. **Name consistently:** `session-0X-[type]-cycle-Y.txt` format
3. **Save to `.log/`:** Place in project's `.log/` directory
4. **Update `project.md`:** Add session summary to checklist
5. **Update `tasks.md`:** If in Session 5 (Implementation), ensure task statuses are current
6. **Commit changes:** Frequent commits throughout, milestone commit after Session 6

Because a single session may span multiple Claude Code sittings, the end-of-session protocol applies when the *phase* wraps — not every individual sitting. Intermediate sittings still commit and push work, but the summary update and log export happen at phase boundaries.

---

## Iteration Strategy

**After Each Cycle:**
- Review Session 6 (Code Review) feedback
- Decide next focus (improvements vs new features)
- Return to Session 4 for the next cycle's Planning
- Default: Continue roadmap
- Flexibility: Incorporate immediate improvements as needed
- Embed lightweight research into Planning if the next cycle touches unfamiliar ground

**Task File Management:**
- `tasks.md` is working state, reset each cycle
- Previous tasks captured in session export and git commits
- No need to archive old task files
