# Project Development Process

This document describes the structured process for developing projects in this learning repository, with heavy AI collaboration throughout.

## Overview

Projects follow a phased approach that balances velocity, learning, and quality. The process emphasizes:
- **Early validation** through MVP development
- **Iterative refinement** based on testing feedback
- **Decision transparency** with documented rationales
- **Learning integration** through hands-on test writing

## Process Flow

```
Session 1: Project Initiation
    ↓
Session 2: Project Architecture
    ↓
Session 3: Project Planning ←─┐
    ↓                         │
Session 4: Implementation     │
    ↓                         │
Session 5: Testing/Catchup ───┘
```

**First iteration:** Complete Sessions 1→2→3→4→5 to build and validate MVP
**Subsequent iterations:** Cycle through 3→4→5 to add features and refinements

## AI's Role

AI (Claude Code) serves as collaborative partner throughout all sessions:

### Sessions 1-3 (Initiation, Architecture, Planning)
- **Interviewing/Facilitating:** Ask structured questions to capture requirements and organize thinking. Ask questions **one at a time** to allow for clear, detailed answers.
- **Collaborating/Proposing:** Suggest architectural patterns, identify risks, propose solutions
- **Decision Support:**
  - Present multiple options with trade-offs
  - Explain alternatives in depth as needed
  - User asks clarifying questions until understanding is sufficient
  - User makes final decision (AI documents it)
  - End-of-session risk review in Session 2 (architecture) for structural concerns that could doom implementation

### Session 4 (Implementation)
- **Claude Code:** Orchestration, process adherence, tracking progress against plan
- **aipack agents:** Code generation across files (minimizes context load on Claude)
- Focus on velocity with quality
- No stopping to explain patterns (covered in Session 5)

### Session 5 (Testing/User Catchup)
- Provide minimal scaffolding to start test writing
- Hands-off while user writes tests independently
- Assist only when user is truly blocked
- Collaborate on test review and gap identification

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

### Session 2: Project Architecture

**Purpose:** Design the technical foundation and MVP scope

**Activities:**
- Define long-term architectural vision
  - File/directory structure
  - Documentation approach
  - Testing strategy
  - Tools, APIs, libraries, technologies
- Identify proof-of-concept needs (critical assumptions to validate)
- Define MVP scope (minimum to validate core idea)
- Plan transition from MVP to full version
- Make architectural decisions with trade-off discussions
- End-of-session risk review

**Decision-Making Process:**
- AI presents architectural options
- Discuss trade-offs, alternatives, implications
- User asks clarifying questions as needed
- User makes final decisions
- Decisions documented with brief rationale

**Output:**
- `architecture.md` created with chosen architecture and brief rationale
- Decision summaries in `project.md` (alternatives, trade-offs, deciding factors)

---

### Session 3: Project Planning

**Purpose:** Break down implementation into actionable tasks

**Activities:**
- Review feedback from previous cycle (if applicable)
- Decide: address immediate improvements OR continue roadmap
- Define high-level phases for current scope
- Break work into atomic chunks (≤10 lines of code where possible)
- Identify opportunities for parallel development
- Identify which tasks are suited for aipack agents (repetitive, file-by-file generation) vs Claude Code (complex logic, debugging, architectural decisions)
- Create `tasks.md` with detailed task list

**Output:**
- `tasks.md` created with tasks organized by status (pending/in-progress/completed)
- Planning summary in `project.md` (objective, scope, phases)

**Note:** Task list defines "done" for this iteration cycle

---

### Session 4: Implementation

**Purpose:** Build the planned functionality with velocity

**Tool Division:**
- **Claude Code:** Orchestration, process adherence, tracking progress against plan
- **aipack agents:** Code generation across files (minimizes context load on Claude)

**Approach:**
- Default to moving quickly while maintaining code quality
- Work on parallel chunks concurrently where possible
- Make frequent commits with clear, concise messages
- Learning pace adjustable based on project priority and user request
- No pausing for explanations (saved for Session 5)

**Output:**
- Implemented code committed to repository
- `tasks.md` updated as work progresses
- Brief summary in `project.md` (dates, actual vs planned work)

---

### Session 5: Testing & User Catchup

**Purpose:** Understand implementation through test writing, ensure quality

**Phase A - User Test Writing (Learning Focus):**
- AI provides minimal scaffolding (test file structure, areas to focus)
- User writes tests independently
- Struggle is part of learning process
- Ask for help only when truly blocked
- AI gives targeted assistance when requested

**Phase B - Test Review (Quality Focus):**
- Review test coverage together
- Identify gaps in validation/regression prevention
- Discuss what additional tests are needed
- Write additional tests as needed (with AI assistance if desired)

**Phase C - Cycle Closure:**
- Create milestone commit
- Document tested areas and feedback for next cycle in `project.md`
- Determine next iteration's focus

**Output:**
- Tests written and committed
- Session 5 summary in `project.md` (areas tested, feedback for next planning cycle)

---

## Project Documentation Structure

Each project follows this structure:

```
projects/project_name/
├── .log/                    # Raw exported conversation logs
│   ├── session-01-initiation.txt
│   ├── session-02-architecture.txt
│   ├── session-03-planning-cycle-1.txt
│   ├── session-04-implementation-cycle-1.txt
│   ├── session-05-testing-cycle-1.txt
│   └── ...
├── project.md               # Persistent checklist/tracker (from template)
├── architecture.md          # Architecture decisions and rationale
├── tasks.md                 # Current cycle's task list (reset each cycle)
└── [project files]
```

**Information Density Hierarchy:**
1. `.log/` files = Full discussion and reasoning
2. `project.md` = Decision summaries with alternatives and trade-offs
3. `architecture.md` = Final architectural decisions with brief rationale

---

## Special Cases

### Architecture Revision
If testing reveals fundamental architectural issues (not just features, but structure):
- Only consider if there's a very clear and obvious problem
- Trigger emergency Session 2 to revise architecture
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
5. **Update `tasks.md`:** If in Session 4, ensure task statuses current
6. **Commit changes:** Frequent commits throughout, milestone commit after Session 5

---

## Iteration Strategy

**After First Cycle (MVP Validation):**
- Review Session 5 feedback
- Decide next focus (improvements vs new features)
- Return to Session 3 for next cycle planning
- Default: Continue roadmap from Session 2
- Flexibility: Incorporate immediate improvements as needed

**Task File Management:**
- `tasks.md` is working state, reset each cycle
- Previous tasks captured in session export and git commits
- No need to archive old task files
