# Architecture

**Last verified against the code:** 2026-08-28 (pre-v1 review).

This describes the system **as built**. Where a decision from the original plan was
reversed, that is stated explicitly rather than quietly dropped — the reversals are
usually the interesting part. `research.md` holds the pre-implementation exploration
and is deliberately frozen as a point-in-time document; where the two disagree, this
file is right and `research.md` is history.

---

## What it is

A personal notes / journal / routines / finances app for a single user, running
offline-first on Android and Linux desktop, syncing through a small server the user
owns. Everything is Rust except the text editor.

Five surfaces ship: **Journal**, **Notes**, **Routines**, **Finances**, **Settings**.
The original plan listed thirteen features (Decisions, Goal Setter, Task Manager,
People Tracker, Locations, Scheduler, Meal Tracker, Knowledge Compounder, Archive);
those were not built and are not scaffolded. Nothing in the codebase is waiting for
them.

---

## Repository layout

| Path | What it is | Rust LOC |
|---|---|---|
| `core/` | All domain logic: events, projections, sync, money, LLM, extraction, auto-import | ~29,800 |
| `server/` | axum HTTP server — sync endpoints, blob/document storage, LLM proxy, update manifests | ~1,400 |
| `tauri-app/src-tauri/` | Tauri v2 shell: 100 `#[tauri::command]` handlers, schedulers, platform glue | ~6,600 |
| `tauri-app/frontend/` | Dioxus 0.7 UI compiled to WASM | ~21,100 |
| `tauri-app/assets/js/editor.js` | CodeMirror 6 editor (the only non-Rust source, ~950 lines JS) | — |

**Two workspaces, not one.** The root workspace is `["core", "server",
"tauri-app/src-tauri"]`. `tauri-app/frontend` is `exclude`d *and* declares its own bare
`[workspace]` table, so it can be built independently by `dx` without dragging the
parent in. The practical consequence is permanent and easy to forget: **no `--workspace`
or `-p` flag run from the repo root can ever reach the frontend.** Every frontend
command must run from `tauri-app/frontend/`. CI has a dedicated job for exactly this
reason.

---

## Event sourcing — the spine

Every state change is an append-only immutable event. Current state is derived by
folding events into read models (projections).

```
Event { id, event_type, aggregate_id, timestamp, device_id, payload: JSON }
```

There are **37 event types** (`core/src/events/types.rs`), each with a typed payload
struct, a `validate_payload` arm, and matching serialize/parse arms.
`event_type_display_roundtrip` currently exercises all 37 — but it does so from a
hand-written array, not a `match`, so the compiler will not notice a 38th variant added
without touching it.

**Five projections** are registered at startup (`src-tauri/src/lib.rs`), all folding the
same event stream:

| Projection | Writes to |
|---|---|
| `NotesProjection` | SurrealDB — journal entries, generic notes |
| `RoutinesProjection` | SurrealDB — routine groups, items, completions |
| `BudgetProjection` | SurrealDB — transactions, accounts, budgets, recurring patterns |
| `AutoImportProjection` | SurrealDB — imported batches and their review state |
| `JournalFile` | a **plaintext hledger file on disk** |

`JournalFile` is the odd one and is deliberate: it is a projection whose side effect is a
file write rather than a database row. The journal file is a regenerable cache — delete
it and `rebuild()` replays the log to reconstruct it. The event log is the only source of
truth.

**Projections must be idempotent and self-healing.** Several arms materialize with
`UPSERT` rather than `UPDATE` specifically because a device can receive an *update* event
for a row it never saw created (the create was lost to an old batch abort). A bare
`UPDATE` silently no-ops in that case, and the symptom is a blank note appearing on the
receiving device with the sync counters reporting success.

---

## Sync

Server-mediated, both directions, over HTTP.

- **Push** is debounced, not periodic. `PushDebouncer::trigger()` wakes
  `pusher::run_loop`, which coalesces a burst of edits into one push after a quiet
  window. There is **no interval fallback** — an append that fails to nudge the
  debouncer does not sync slowly, it does not sync at all.
- That property makes "append and nudge" a pairing you cannot forget safely, so it is
  enforced structurally rather than by convention: every write goes through
  `commands::shared::{append_new_and_apply, append_batch_and_apply}`, and the test
  `no_command_appends_events_directly` scans the source tree to keep it that way.
- **Pull** is a separate `PullScheduler` on an interval, with a warmup.
- `NetworkMonitor` probes connectivity; `RetryEngine` handles backoff; `StatusReporter`
  drives the UI's sync indicator.

**Sync is last-write-wins, and there is no merge.** The original plan claimed event
sourcing gives "no conflicts, no last-write-wins" — that is wrong as built, and the
correction matters. Events are facts and never conflict *as events*, but two devices
editing the same entry produce two valid events, and the projection applies them in
order: the later one wins and the earlier edit is gone. No CRDT, no three-way merge.
This is accepted for a single-user system where concurrent multi-device editing of one
entry is synthetic. The mitigation that does exist is narrower: a dirty-protect sticky
flag stops a arriving sync from clobbering text while the user is actively typing in it.

---

## Money

**No `hledger` CLI, no subprocess.** The plan called for shelling out to hledger; the
implementation parses and computes in-process via the `ledger-parser` + `ledger-utils`
crates (`core/src/ledger.rs`). This was validated against a real 5,826-transaction
journal on both desktop and Android arm64, byte-identical results. Avoiding a native
binary dependency on Android is what made it worth doing.

The chain, and the reason it is reviewed as one unit:

```
journal_import.rs ─┐
capture commands  ─┼─→ TransactionRecorded event ─→ BudgetProjection  (SurrealDB rows)
statement CSV     ─┘                              └─→ JournalFile      (hledger text)
                                                        │
                                          ledger.rs (parse) ─→ balances.rs ─→ dashboard
```

One format is written by `JournalFile` and read back by `ledger.rs`. A render/parse
split like that is where this codebase has repeatedly produced its worst bugs, so
`tests/golden_reconcile.rs` guards the whole path end-to-end and is expected to be green
on **every** money-chain change.

Accounts follow a MECE grammar — `Assets:<Registration>:<Commodity>` — with institution
and product carried as posting *tags* rather than account-name segments.

---

## LLM and document extraction

Two separate trait-based abstractions, both with real second implementations (so the
seam is load-bearing, not speculative):

| Trait | Implementations |
|---|---|
| `LlmClient` (`core/src/llm/`) | `GeminiClient`, `OpenAiCompatClient` |
| `DocumentExtractor` (`core/src/extraction/`) | `GeminiExtractor`, `OpenAiCompatExtractor`, `NullExtractor` |

Extraction routes on a MIME-derived hint with a user override where MIME is ambiguous.
Deterministic pre-processing (`core/src/preprocess/`) runs before anything reaches a
model.

**LLM tools bind to `core` functions, never to Tauri commands.** Tools live in
`llm/tools.rs` (`ToolDef`, dispatched through `pipeline.rs`) and call core directly. A
`#[tauri::command]` exists only to cross the frontend IPC boundary and is the wrong thing
for a tool to reuse — it stringifies `Decimal`s for transport and drops fields the view
type doesn't carry.

**Nothing an LLM produces is committed without review.** Extracted drafts land in a
`pending` inbox and require an explicit user commit. For the email path this is a
security control, not a convenience: message bodies and PDF attachment text are
attacker-controlled and go to the model verbatim, so prompt injection can choose the
amounts. The review step is the only thing between a crafted email and a fabricated
ledger entry, and there is no sender authentication.

---

## Auto-import

`AutoImportSource` has four production implementations — `CsvSource`, `RestSource`,
`SubprocessSource`, `ImapSource` — plus `NullSource` for tests. All converge on a shared
`to_proposed_event()` tail, and per-kind branching exists in exactly one place
(`config.rs::build_one`).

IMAP is deliberately **not** config-driven (`validate()` rejects `"imap"`): it needs a
different credential shape and is wired through a separately compiled path.

The whole subsystem is behind core's `auto-import` feature flag, which pulls the IMAP and
native-TLS dependencies. Tauri clients build with default features **off** — this keeps
`openssl-sys` out of the Android dependency tree. The scheduler runs in `omni-me-server`,
not on the client.

> Feature-flag gotcha, because it has caused a false conclusion once already: cargo
> unifies features **per invocation**. `cargo test -p omni-me-core` alone runs 513 tests;
> `cargo test -p omni-me-core -p omni-me-server` runs 609, because `server` depends on
> core with `auto-import` enabled. Splitting that one command into two silently loses
> ~96 tests with nothing failing.

---

## Frontend

Dioxus 0.7 compiled to WASM, five pages plus a shared component set
(`components/primitives.rs` is the design system: `Button`, `Card`, `Banner`,
`PageHeader`, `SegmentedNav`, `TextInput`/`INPUT_CLASS`, `IconButton`, plus an icon set).

**`bridge.rs` is the entire IPC boundary** — 98 `invoke_*` wrappers, each with a
`#[cfg(feature = "mock")]` twin. The `mock` feature swaps the whole backend for stubs so
the UI can be driven in a plain browser. Four of those mocks are stateful
(`MOCK_ACCOUNT_OVERRIDES`, `MOCK_PAUSED`, `MOCK_SOURCE_CONFIGS`, `MOCK_LLM_CONFIG`) and
re-implement backend CRUD semantics by hand: type drift between mock and real is
compiler-caught, **behavioural drift is not**.

`types.rs` hand-mirrors ~56 backend structs. There is no shared crate and cannot be one —
importing `core` into the WASM bundle would drag a database driver into the browser.
Serde enforces the mirror at runtime, on the first call that uses a drifted field.

**The editor is JavaScript.** CodeMirror 6 in `assets/js/editor.js`, bundled by esbuild,
talking to Rust over `wasm_bindgen` externs. Content and cursor flow **push**-style
(`on_change` / `on_cursor` callbacks), not pull.

---

## Platform

| Concern | Android | Desktop |
|---|---|---|
| Updates | Hand-rolled OTA — the server publishes a manifest + signed APK under `/updates`; the app downloads to its cache dir, verifies sha256, and hands off to a Kotlin `InstallBridge` which fires the system package-installer intent | Tauri updater plugin |
| Share sheet | `MainActivity.kt` writes shared bytes + a metadata sidecar to `filesDir`; the frontend drains it via `take_pending_share_intent` on mount | n/a |

Tauri's updater plugin does not support mobile, which is why the Android half exists at
all. Kotlin overrides live under `src-tauri/gen/android/` (gitignored — regenerated) with
committed source copied in by `build.rs`.

Build via the committed scripts — `tauri-app/scripts/desktop-build.sh` and
`android-build.sh` — not by hand-rolling `cargo tauri build`. `assert-no-mock.sh` greps
the bundled artifact for a sentinel string that is compiled in whenever the `mock`
feature is on, because a mock build shipped in an APK once.

---

## Server and deployment

axum, seven route modules: `sync`, `notes`, `documents`, `blobs`, `llm`, `auto_import`,
plus health/updates. It holds its own `ProjectionRunner`, so the server is not a dumb
relay — it folds the same events into its own read models.

Runs on a Hetzner box (the plan's DigitalOcean choice was dropped — payment was
rejected). Reachable over Tailscale rather than the public internet.

**Authentication is a bearer token that fails open.** If `[server].auth_token` is absent
from `credentials.toml`, the server starts anyway and logs a loud warning with a
suggested generated token. That is a deliberate choice for a single-user box that must
not brick itself on a bad config, and it is only defensible because Tailscale is doing
the real perimeter work. It matters more than it looks: `PUT /llm/config` writes an API
key and `POST /auto_import/sources` registers a **subprocess** source — i.e. command
execution on the box.

SurrealDB is pinned to **3.0.4** in lockstep across both repos, storage engine
`kv-surrealkv`, embedded single-file on device and server alike.

---

## Reversals from the original plan

| Planned | Actual | Why |
|---|---|---|
| `hledger` CLI via subprocess | In-process `ledger-parser` + `ledger-utils` | No native binary dependency on Android |
| Mindee OCR | Gemini multimodal | One provider for both LLM and extraction |
| Paisa for visualization | Custom SVG charts in-app | Avoids a second hosted service |
| DigitalOcean | Hetzner | DO rejected payment |
| "No conflicts, no last-write-wins" | Last-write-wins, no merge | Events don't conflict; *projections* still overwrite |
| 13 features | 5 | Scope discipline; the rest were never scaffolded |
| SQLite fallback | Not needed | SurrealDB embedded held up |

---

## Accepted trade-offs

These are settled decisions, documented in code at their sites. They are not open
questions, and a future review should not re-file them as findings.

- **Sync is last-write-wins.** Merge deferred; concurrent same-entry multi-device editing
  is synthetic for one user.
- **`⟦…⟧` completion tokens leak into export / LLM / search.** Accepted; the alternative
  designs were worse.
- **`auto_close` scan→emit is TOCTOU**, and `is_complete` is any-non-empty-wins. Both
  documented in code specifically so nobody "fixes" them.
- **The bearer token fails open when unconfigured** (see above).
- **`serde-saphyr` replaced `serde_yml`** after a billion-laughs exposure; input is capped
  at 64 KiB.

## Known constraints

- The frontend's Playwright loop uses Chromium; the desktop app runs **webkit2gtk**.
  These disagree on native form controls, scrollbars and date pickers, so control-level
  styling cannot be signed off from a Playwright screenshot — that has shipped a broken
  fix once.
- The `mock` bridge only returns *today's* journal entry and never `closed`/`complete`,
  so day-completion and closed-day read-only behaviour can only be exercised on a real
  backend. Cover that logic with unit tests instead.
- There is a known SurrealDB tempfile race in the test suite: reproduced, passes on
  rerun, not chased.
