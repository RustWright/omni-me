# Project Development Process

> **MIRROR FILE — KEEP IN SYNC.** The canonical version of this document lives at `setup_files/PROJECT_PROCESS.md`. **Any edit made here must be propagated to the canonical (and vice versa).** If you only update one copy, the two will drift — Cycle 2 Session 6 hit this when the project copy was stale at process-revision time. Treat this banner as a tripwire: if you touched the doc, also update the other one before you finish the session.

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
- **Identify post-system triggers within the cycle** (see [Post System](#post-system) below): flag tasks likely to land a public-facing **logbook entry** — explicit "this is logbook-worthy" annotation in the task list so the question is settled at planning time, not at write-up time. Also flag features that may warrant a **portfolio demo** (`{{ demo() }}` shortcode in §6 of the logbook entry).
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
- **Capture cadence for post-system triggers:** at feature-landing commits, pause to draft logbook capture sections (§1-5 via `logbook init/what/why/scope/note`) while context is freshest. Batch §6 evidence (`logbook exec`, `logbook screenshot`, screenshots requiring UI captures) for end-of-day or end-of-cycle when they would otherwise slow other development. Editorial guidance: `mylearnbase/editorial/logbook.md`.
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

**Phase D — Post-system pulls (discovery, no fix code):**

A separate phase from the review-and-resolve loop. Two pulls:

1. **Cycle-close curiosity review.** Walk `<project-repo>/.curiosities/<cycle-id>.md` end-to-end (see `~/.claude/CLAUDE.md` § Curiosity Capture for how the file gets populated). For each entry, ask: still hold attention? still feel unfinished? lend itself to interactive demo? Survivors become candidates for **concepts posts** on mylearnbase. Most won't survive — resolved during work, interest faded — and that's expected. A cycle with zero survivors is normal.
2. **Portfolio-demo identification.** Review which features from the cycle warrant a `{{ demo() }}` shortcode demo as logbook §6 evidence. Pick on coolness + implementability (static-site + WASM-islands constraints from `architecture.md`).

Both pulls feed into post drafting **outside** the cycle process — they're surfacing work, not write work. Post drafting happens via `/create-post` when the author has time, not as part of Code Review.

**Output:**
- `reviews/YYYY-MM-DD-*.md` per-perspective review documents (created Phase A, annotated with disposition during Phase C)
- `reviews/YYYY-MM-DD-test-gaps.md` test-gap proposal (created Phase B, annotated during Phase C)
- Fix commits landed on the cycle's branch/main as triage progresses
- Deferred items recorded in `project.md` for the next cycle's Planning
- `<project-repo>/.curiosities/<cycle-id>.md` annotated with survivor markers (Phase D); demo-candidate list noted in `project.md` for the next cycle's Planning to pick up

---

## Project Documentation Structure

Each project follows this structure:

```
projects/project_name/
├── .log/                    # Raw exported conversation logs (gitignored here, parent-synced)
│   ├── session-01-initiation.txt
│   ├── session-02-research.txt
│   ├── session-03-architecture.txt
│   ├── session-04-planning-cycle-1.txt
│   ├── session-05-implementation-cycle-1.txt
│   ├── session-06-code-review-cycle-1.txt
│   └── ...
├── .curiosities/            # Cycle-scoped curiosity captures (gitignored here, parent-synced)
│   ├── cycle-1.md
│   ├── cycle-2.md
│   └── ...
├── project.md               # Persistent checklist/tracker (from template)
├── research.md              # Research findings (first cycle)
├── architecture.md          # Architecture decisions and rationale
├── tasks.md                 # Current cycle's task list (reset each cycle)
├── reviews/                 # Per-cycle code review findings
└── [project files]
```

Both `.log/` and `.curiosities/` are **gitignored in the project repo** so raw conversation logs and in-progress curiosity captures never enter the project's public history. They sync to the parent `productive_learning` repo at session end (see `~/.claude/CLAUDE.md` § Session Sync Protocol Step 2) where they're tracked privately. The rest of the structure is tracked in the project's own git history.

**Information Density Hierarchy:**
1. `.log/` files = Full discussion and reasoning
2. `project.md` = Decision summaries with alternatives and trade-offs
3. `research.md` / `architecture.md` = Canonical technical decisions with brief rationale
4. `reviews/` = Code review findings per perspective per cycle
5. `.curiosities/` = Cycle-scoped accumulator for concepts-post triggers (LLM-appended during cycle work; reviewed in Session 6 Phase D)

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
7. **Sync to parent repo:** see `~/.claude/CLAUDE.md` § Session Sync Protocol Step 2 — copies `.log/` and `.curiosities/` into `<parent>/logs/<project-name>/` and `<parent>/curiosities/<project-name>/`. The curiosity log feeds the Session 6 Phase D cycle-close review pass.

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

---

## Post System

Some project work produces public-facing posts on [My Learn Base](https://mylearnbase.com). Five forms cover the post types, each with its own tools, editorial standard, and authoring rhythm:

| Form | What it captures | Trigger point in the cycle |
|---|---|---|
| **logbook** | A feature you built; evidence-of-shipping | Capture during Session 5 (Implementation); publish when feature lands |
| **concepts** | A concept you came to understand via an interactive demo | Survives cycle-close curiosity review in Session 6 Phase D |
| **workflows** | A process/workflow doc (this very doc is one) | Republish whenever the source doc changes |
| **opinions** | A take, and the take is the point | Spontaneous — outside the cycle process |
| **resources** | Curated external references | Project-end or theme readiness — outside the cycle process |

**Authoring entry point:** `/create-post` (global Claude Code slash command) prompts for form first and routes to the editorial doc.

**Editorial source of truth** (in `mylearnbase/editorial/`):

- `logbook.md` — features you built
- `concepts.md` — interactive demos teaching concepts
- `workflows.md` — process/workflow docs synced from source docs
- `opinions.md` — takes with the take as the point
- `resources.md` — curated external references
- `tagging.md` — cross-form tagging strategy (style conventions, decision rules, anti-patterns; applies to every form)

Each editorial doc covers when to use, tools, section structure with LLM-vs-author ownership, anti-patterns, and authoring rhythm. **Do not re-encode these rules elsewhere** — drift is the failure mode.

**Curiosity capture** runs continuously across all projects. See `~/.claude/CLAUDE.md` § Curiosity Capture for the mechanism. The cycle-close review pass in Session 6 Phase D walks the resulting log.
