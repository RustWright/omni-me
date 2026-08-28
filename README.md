# omni-me

A personal notes, journal, routines and finances app. Offline-first, event-sourced,
running on Android and Linux desktop from one Rust codebase, syncing through a server you
run yourself.

Built as a single-user system for one person's actual daily use — not a product, and the
design reflects that: it optimises for durability of the data and honesty about what has
actually been verified, rather than for onboarding a second user.

---

## What it does

| Surface | |
|---|---|
| **Journal** | One entry per day, autosaving, with a calendar and closable days |
| **Notes** | Free-form notes with tags and search |
| **Routines** | Recurring checklists that log completion timestamps |
| **Finances** | Double-entry bookkeeping over a plaintext ledger — capture receipts by photo, PDF or email, import bank statements, reconcile, budget, and track net worth |
| **Settings** | Sync, currency, LLM provider, auto-import sources, updates |

The finance side is the substantial half. Transactions live as events, project into both a
queryable database and a plaintext hledger-format journal file, and are read back through
an in-process ledger engine to compute balances.

---

## How it's built

**Everything is Rust except the text editor.**

```
core/                    domain logic — events, projections, sync, money, LLM, extraction
server/                  axum HTTP server + its own projections
tauri-app/src-tauri/     Tauri v2 shell, 100 IPC command handlers
tauri-app/frontend/      Dioxus 0.7 UI, compiled to WebAssembly
tauri-app/assets/js/     CodeMirror 6 editor (the only non-Rust source)
```

**Event sourcing is the spine.** Every state change is an append-only immutable event;
current state is a fold over the log. Thirty-seven event types feed five projections — four
writing to SurrealDB, and one whose side effect is writing a plaintext ledger file to
disk. The ledger file is a regenerable cache: delete it and it replays.

That choice is load-bearing rather than decorative. Because the log is the only source of
truth, a projection bug is recoverable by rebuild, schema changes are reinterpretations
rather than migrations, and sync is a matter of exchanging facts.

**A few decisions worth naming:**

- **No `hledger` subprocess.** Parsing and balance computation happen in-process via
  `ledger-parser` / `ledger-utils`, validated against a real 5,826-transaction journal on
  desktop and Android arm64 with byte-identical results. Shipping a native binary to
  Android was the thing worth avoiding.
- **Push sync is debounced, not polled**, with no interval fallback — so "append an event"
  and "wake the pusher" are paired inside one helper, and a source-scanning test keeps them
  paired.
- **Nothing an LLM extracts is committed automatically.** Drafts land in a review inbox.
  For the email-ingest path this is a security control, not a nicety: message bodies are
  attacker-controlled and reach the model verbatim.
- **The frontend is a separate cargo workspace** so it can be built independently — which
  also means no `--workspace` flag from the repo root ever reaches it.

Full detail, including the decisions that were reversed during implementation, is in
[`architecture.md`](architecture.md).

---

## Building

Requires a Rust toolchain, Node (for the editor bundle), and the
`wasm32-unknown-unknown` target. On Debian/Ubuntu the Tauri shell additionally needs
`libwebkit2gtk-4.1-dev` and `libgtk-3-dev`.

```bash
# Backend tests — core's 609 plus the server suites, in ONE invocation (see note)
cargo test -p omni-me-core -p omni-me-server

# Tauri app
cargo test -p omni-me-app

# Frontend — from its own directory, it is a separate workspace
cd tauri-app/frontend && cargo test

# Desktop bundle (release; forwards extra args to `cargo tauri build`)
tauri-app/scripts/desktop-build.sh

# Android APK — [debug|release], default release
tauri-app/scripts/android-build.sh
```

> Naming both backend packages in one `cargo test` invocation is deliberate: cargo unifies
> features per invocation, and `server` depends on `core` with its `auto-import` feature
> on. Split the command in two and core quietly drops from 609 tests to 513.

UI work has its own loop — browser + mock bridge + Playwright — documented in
[`UI_WORKFLOW.md`](UI_WORKFLOW.md), with the interaction checklist in
[`ui-checklist.md`](ui-checklist.md).

---

## Repository layout

This is the **public engine**. A private overlay repository supplies personal
institution-specific import drivers and deployment configuration; the public side is
built and tested without it and has no dependency on it.

| File | |
|---|---|
| [`architecture.md`](architecture.md) | The system as built, including reversals from the original plan |
| [`UI_WORKFLOW.md`](UI_WORKFLOW.md) | How to develop and verify the UI |
| [`ui-checklist.md`](ui-checklist.md) | Interaction checklist |
| [`SUBPROCESS_SOURCE_CONTRACT.md`](SUBPROCESS_SOURCE_CONTRACT.md) | Contract for external import drivers |
| [`SOURCE_REAUTH_DESIGN.md`](SOURCE_REAUTH_DESIGN.md) | Re-authentication flow for import sources |
| `research.md` | Pre-implementation exploration — deliberately frozen, historical |
| `PROJECT_PROCESS.md` | The development process this project follows |

---

## Status

Pre-v1, in the release-gate code review. The app is in daily use by its author.

Known and accepted: sync is last-write-wins with no merge (concurrent multi-device edits
of the same entry overwrite); a handful of subsystems are verified only against a real
device rather than in CI. Both are documented at their sites in the code rather than left
for a reader to discover.

## License

Not yet chosen.
