# Tasks — post-v1 daily use

**Status:** v1.0.5, released and in real daily use on the personal phone and `surface`
(the Linux desktop). Both devices sync against the Hetzner box.

**Reconciled against git 2026-09-05** (twice — the second pass closed the three on-device
items, deferred finances indefinitely, and set the three-item sequence). This file carries
**open work only**. Everything
closed in that pass — including its original verification notes — is in
[`.archive/post-v1/tasks-completed.md`](.archive/post-v1/tasks-completed.md); work up to
the v1.0.0 cut is in [`.archive/v1.0.0/tasks-completed.md`](.archive/v1.0.0/tasks-completed.md).

## How to read this file

- **It is not a state snapshot.** It went stale for four sessions because nothing warns
  when it drifts, unlike `NEXT.md`, which SessionStart prints and canaries. Check the
  `Reconciled against git` date above: work landed after it has not been folded in.
- **`NEXT.md` outranks this file** on what to do next. This is the inventory; that is the
  handoff.
- **Every item below was closed or kept against evidence in the repo** — a commit, a code
  path, or a confirmation on real hardware — never against its own prose. Items that
  contradicted the code were archived; items whose prose merely *claimed* a fix were kept
  if only a device could settle them (see the first section).

Size tags: [XS] ≤30min · [S] ~1h · [M] ~2-3h · [L] ~4-6h · [USER] user action

## Standing constraints

Carried out of the old header, where they were buried under finished roadmap narrative.
These still bind; the rest of that header is in the post-v1 archive.

- ~~**Nothing new at the top level until journal, routines and finances work end-to-end.**~~
  **Superseded 2026-09-05.** That rule gated LLM chat behind finances, and finances is now
  deferred indefinitely — so the gate could never open and would have blocked the user's own
  stated direction forever. Journal and routines *are* working end-to-end and in daily use;
  they were the part of the rule that mattered. Replaced by the sequence below.
- **Dogfooding is the test harness.** Real daily friction is the primary bug-finder, and
  scope creep from it is expected and has a home in this file.
- **Sequential work, no parallel worktrees** — one three-agent run cost ~$40 and 17GB.
  Releases build in CI; ⚠️ **this box OOM-kills even dev builds** — `CARGO_BUILD_JOBS=1`, one
  crate at a time, never two cargo processes at once.

---

## The agreed sequence (user, 2026-09-05) — in order, one at a time

Set after the user stopped work on finances. **Each gets planning before building**, per the
defer-major-phases rule; do not run ahead to the next one.

1. **Feedback capture** — planned and **Stage 1 built 2026-09-05**. Capture at the point of
   friction with app context attached; stored as a `FeedbackCaptured` event, read back over
   `GET /feedback`. Detail in "Open — from daily use" below. This was first because it is how
   every later item gets its bug reports. **Still open under it:** Stage 2 (the diagnostic
   ring buffer) and the live-box end-to-end.
2. **Generalization** — plan, then decide *or* address. The open-core engine still carries
   the user's own choices as hardcoded structure: the journal template's three reflection
   keys, the statement layout strings, `FORCE_GENERIC_DIRS`. A decision to defer is a valid
   outcome here; what is not valid is leaving it undecided.
3. **AI / LLM / ML integration — detailed planning and rethinking.** How to integrate, host
   and *securely* use AI in this app for the best value at the least cost. Explicitly a
   re-examination, not an implementation of the existing design: `DocumentExtractor`/Gemini
   was queued for re-evaluation on 2026-09-05 rather than accepted. Related backlog: LLM chat
   that executes commands (Cycle 5 filing), feature toggles, `llm/` as it stands today.

---

## Awaiting on-device confirmation — none open

All three cleared by the user on 2026-09-05, from real use rather than a staged test:
note/journal body edits and ledger transaction edits both propagate across devices (the
session's own feedback dump was written on mobile and copy-pasted from desktop; committed
Unmatched auto-import transactions showed as committed on the other device), and Android
predictive text commits on space. The resolved narratives are in the post-v1 archive.

---

## Finances — DEFERRED INDEFINITELY (user, 2026-09-05)

⛔ **Do not start finance work. Do not propose it. Do not "just fix" an item below.**
The user stopped using the finances section and has deferred all further work on it with no
date. What exists **ships as-is**; nothing here is a bug queue awaiting attention.

This overrides THE BAR rather than satisfying it: the earlier rule ("the finance tab stays
offline until import beats the old system's") described a gate to be *cleared*, and there is
now no intent to clear it. What survives from that decision set is only the **state** — the
finance tab stays offline, both bank sources stay OFF, categorization stays deferred to
`Unmatched`. Nothing turns on without the user saying so.

The items below are kept **as a record of where the work stopped**, not as a backlog, so that
whoever picks it up later — possibly with the LLM push, which may change the shape of the
problem entirely — starts from what was established rather than re-deriving it. The full
decision set is in the overlay's `CATEGORIZATION_DEFERRAL.md` and `IMPORT_PARITY.md`.

**Known gap at the stopping point:** the CIBC CSV format (highest transaction volume, four
accounts) has **no oracle at all** — its export carries no balance column. Each of the four
CIBC dirs in the pCloud backup holds 2 unexamined PDFs; if those are statements, the rendered
parser built 2026-09-05 would close the gap. Never checked.

- [ ] **Reach import parity with paisa's seven importers.** Parity map is written (overlay
  `IMPORT_PARITY.md`; institution names are private). **Four of the seven are now covered**
  as of 2026-09-05: the old comma-splitting `statement_csv.rs` is deleted, imports run
  through `core/src/statement/`, and both rendered-PDF layouts parse — verified over 136 real
  files with zero self-check failures. Remaining: the two other CSV institutions and the
  investment/holdings shape, which is a genuinely different problem (positions, not cash
  rows). paisa's importers are in the pCloud backup with the corpus in sibling dirs. [M]
- [x] **CSV import adopts the document path's refusal gate.** Done 2026-09-05 (user chose to
  unify). `StatementParse::import_blockers` is the single policy, one `ImportStatementResult`
  serves every format, and the UI has one report panel. A new `Verifiability` keeps "checked
  and passed" separate from "nothing to check" — the chequing export has no balance column, so
  it clears the gate by offering none, and the panel says so in words.
- [ ] **Statements as the source-of-truth health check** (user, 2026-09-03). End-of-month
  statements are definitive for account state, so ingesting them gives a closing-balance
  oracle to assert auto-pulled data against — catching reversed or missed transactions that
  reconcile silently today. Extends the existing balance-check on the finances page, and
  gives the document-archiving plan a reason to keep collecting statements. Check whether
  statement auto-download is feasible per institution. [L]
- [ ] **Categorization classifier — blocked, not scheduled.** No classifier ships without
  posting provenance (`src:rule/<id>` / `src:model/<ver>` / `src:human`); the exit path is
  appending `TransactionUpdated` with a full `postings` array. Evaluate on **abstention**,
  not accuracy. ⚠️ The existing ledger is **not** labelled training data. [L, gated]
- [ ] **Crypto is modelled monthly — ask whether that was intended.** The statement audit
  found 60 of 62 daily rows against 4 monthly aggregates; balances are exact but counts
  differ. Still the only open finding; the audit now covers 24 CSV + 136 rendered statements
  and everything else is clean. [XS, question]

---

## Open — from daily use

### Editor and mobile

- [ ] **Cursor still lands behind the soft keyboard and behind the open drawer** (user,
  2026-09-04, on v1.0.5). Distinct from the top-bar self-toggle loop, which **is** fixed —
  `HEADER_TOGGLE_COOLDOWN_MS` in `main.rs` stops the header's own 300ms height animation
  firing `onscroll` and driving its next toggle. What remains is the cursor's position
  relative to the visual viewport: nothing listens to `window.visualViewport`, so a keyboard
  that shrinks the viewport without a resize event leaves CodeMirror scrolling against a
  stale height. Reproducible by the user on demand. [M, editor]
- [ ] **5.4** Typing-feel polish — open bucket, populated from the friction log as daily use surfaces it. [—]

### Finances

- [ ] **Swipe between Overview / Ledger / Analyze** (user, 2026-09-03) — today the sub-nav
  needs a scroll back to the top and a tap. Swipe handling already exists twice in the app:
  the app-shell nav drawer (`components/nav.rs`) and the journal calendar drawer
  (`pages/journal.rs`, right-edge anchored). `pages/finances.rs` has none. The user is
  reserving judgement on the sticky sub-nav until a week or two of use — if the comparison
  to a native swipe is still nagging by then, this ships. [S, frontend]
- [ ] **On-device sync feels laggy + the open view doesn't always live-refresh** (on-device test, 2026-08-14). Auto-sync **works both directions without the manual button** — validated live: a fresh phone backfilled all **12,643** events (device audit `total=12643, ever_synced=true`), and edits propagate desktop↔phone on their own. Two UX gaps surfaced: **(1) latency** — inbound edits take up to ~20s: the server has no push channel so receivers POLL (`core/src/sync/puller.rs:27` `DEFAULT_PULL_INTERVAL=20s` + 4s warmup; a network-online accelerator nudge exists but steady-state is the 20s poll). Tunable (shorter interval = more requests) or add a push/SSE channel (bigger build). **(2) live-refresh gap** — after an auto-pull applies, the backend emits `sync:applied` (`tauri-app/src-tauri/src/lib.rs:389-402`, only when `pulled>0`) → frontend bumps `sync_epoch` → subscribed views (journal/notes/routines/finances all read it) refetch; BUT the currently-open view **sometimes** stays stale until you navigate away+back (forcing a remount refetch). Suspects: the `sync:applied` nudge not reliably reaching the specific open component, or the editor **dirty-protect** (from `aa41789`, prevents live-clobber-while-typing) over-suppressing the body refresh even when not actually dirty. Needs frontend diagnosis. **Both are polish, NOT blockers — core sync + the 306/308 fixes are validated on-device.** [M, frontend]
  **Narrowed 2026-09-05:** the specific reported case — approving an unmatched auto-import
  transaction not refreshing the ledger until a manual Apply — was fixed in `638d709`
  (`sync_refresh.rs` + `finances.rs`). What remains is the general 20s poll latency and the
  open-view live-refresh gap.

### Platform and onboarding
- [ ] **A "fresh install" on a machine that ran an older build silently inherits that build's
  state, and there is no in-app way to reset it.** Hit on the go-live desktop (`surface`) minutes
  after installing 1.0.3: the app opened onto a **garbage note from April** and reported the
  **wrong box address**. Both traced to files the installer never touches, in
  `~/.local/share/com.omni-me.app/`: a `server_url` holding **`http://localhost:3000`** from a
  March prototype run, and a March-era `local.db`. The persisted `server_url` **takes precedence
  over the compile-time `OMNI_DEFAULT_SERVER_URL`**, so the machine could never reach the box —
  which is also why it pushed nothing and the freshly-seeded box stayed uncontaminated (verified:
  only the import device and the phone appear on it).
  **Fix applied for now = delete the app data dir and relaunch.** Proven on `surface`: new
  `device_id`, `ever_synced=false, total=0` at boot, and a `server_url` resolved from the
  CI-baked default (the real box, not localhost), then a clean backfill. Note a pure backfill
  writes **no** events, so
  the box shows nothing from that device — absence there is not evidence of failure.
  **The product gap is the real item.** Anyone onboarding a second device that ever ran an older
  build hits this, with no affordance short of `rm -rf` on a path they have to know. Worth an
  in-app **reset local data + re-sync** action (Settings), and worth deciding whether a persisted
  `server_url` should really outrank a compile-time default that changed under it. [S–M]
  **Partially resolved 2026-09-05:** the in-app affordance now exists — Settings carries a
  **Danger Zone** with a typed-confirmation `wipe_all_data` command
  (`commands/routines.rs`), so `rm -rf` on a path the user has to know is no longer the only
  option. **Still open:** whether a persisted `server_url` should outrank a compile-time
  default that changed under it. That precedence is what made the machine unreachable, and
  wiping data does not answer it.
- [ ] **Credential artifacts live OUTSIDE the app data dir, so the go-live clean slate missed
  them.** The same inventory found `~/.config/omni-me/credentials.toml` (bank credentials, Jun 14)
  and `~/.local/share/omni-me/ws-session.json` (brokerage session, Jun 15) sitting on the desktop
  machine — both from prototype-era local auto-import runs. In the current architecture these
  belong **only on the box** (`/etc/omni-me/credentials.toml`, mounted read-only and uid-locked;
  the session file on the server volume): auto-import is server-side, so a client has no use for
  either. Deleted on the user's instruction 2026-08-31, along with a dead `com.omni-me.poc` cache
  dir — and **the desktop app was confirmed still running normally afterwards**, which turns "the
  client doesn't need these" from an architectural claim into an observed fact. Contents were
  never opened — credential files are not inspected, even diagnostically.
  **Keep for the wipe runbook:** a clean-slate scope written as "the app data dir + the box"
  is incomplete. `core::credentials::default_path` and `auto_import::config` resolve under
  `$XDG_CONFIG_HOME/omni-me/`, which no data wipe touches. [XS, doc + runbook]
  **Partially resolved 2026-09-05:** the **box-side** paths are documented in the overlay
  (`SETUP.md`, `deploy/README.md`). The client-side strays this item is actually about —
  `$XDG_CONFIG_HOME/omni-me/credentials.toml` and `~/.local/share/omni-me/ws-session.json`
  on a machine that once ran a prototype — are still undocumented in the wipe runbook.

- [ ] **Testing without touching live data** (user, 2026-09-03). The data is no longer
  disposable: ~12k events on the box plus local state on two daily-driver devices. Writing
  test data to the box during a test would mean needing a way to tell test from real
  transactions, which the user has ruled risky. Wants a way to exercise a build against
  real-shaped data without writing to production — export real data to test against is
  acceptable; writing back is not. **This is infrastructure, not a feature**, and it gates
  how safely everything else on this list can be fixed. [M]
- [~] **In-app feedback capture from the page where the issue happens** (user, 2026-09-03).
  **= ITEM 1 of the agreed sequence. Planned and BUILT (Stage 1) 2026-09-05.** Plan:
  `~/.claude/plans/lets-continue-eventual-aurora.md`. Remaining: Stage 2 below, and the
  live-box end-to-end.

  **The design fork is CLOSED — feedback is an event.** `EventType::FeedbackCaptured` +
  `FeedbackCapturedPayload` (only `feedback_id` and `body` required, so capture can never fail
  validation mid-friction), **no projection** — every projection ignores it via `_ => Ok(())`.
  Read back by `queries::list_feedback`, served as markdown by `GET /feedback`, pulled by
  `scripts/pull-feedback.sh`. The tagged-note option was rejected on merits, not cost: a note
  lands in the notes list, note search, the Obsidian export and the LLM derive pass, and
  `generic_notes.tags` is written **only** by `on_llm_processed` — so a typed `tags: [feedback]`
  is not queryable at all. "Pluggable" needed no plugin seam: the destination is whatever calls
  the read endpoint.

  ⚠️ **The 2026-09-05 survey's "strongest design input" — that the user copy-pastes a dump from
  mobile — was RETIRED by the user the same day**: it was a crutch for having no system, not a
  requirement. Do not reinstate note-sync-as-transport reasoning from the archived survey.

  **Screen context is describe-on-demand** (`frontend/src/screen_context.rs`), not a widened
  continuity store: the store persists what would be *lost*, a report wants what was *shown*.
  Journal and Notes publish describers; other pages report position only until they adopt one.
  **Rule: describers summarise, never quote** — Settings must report its section and nothing
  else, since it holds the server-token field.

  **Verified:** 38 core tests, 4 route tests, 94 frontend tests, 3 src-tauri architectural
  guards, both wasm clippy configs, and a Playwright pass at 390 + 1280 with **0 console
  errors** — modal opens over the live page, context list renders screen + build + unsaved-draft
  length, the draft line drops and restores, send returns an id. [M]

- [ ] **Stage 2 — the diagnostic ring buffer.** A report that says "it broke" with no error trail
  is barely better than the dump it replaces. Nothing records panics, console errors or failed
  `invoke` calls today; `tracing_subscriber::fmt()` goes to stdout, which on Android is logcat —
  invisible in-app and gone when the process dies. Needs: a bounded (~50) frontend ring buffer
  fed by a wasm panic hook, a `console.error`/`warn` tap and a failed-`invoke` tap; plus
  `get_recent_events(limit)` (`EventStore` has `get_since`/`get_since_by_device`/
  `get_by_aggregate` — none answer "last N by timestamp for this device"). The payload fields
  `recent_errors` / `recent_events` already exist, so this is not a wire-format change. [M]

- [ ] **Feedback end-to-end against a live box.** Everything so far ran against the mock bridge.
  The cross-device leg is the only part that proves the loop: capture on the phone, then
  `GET /feedback` from the desktop and see that report. [S]

### Release engineering
- [ ] **Desktop DOES flash white for ~320ms — but `backgroundColor` is NOT the culprit and the
  fix is a different layer.** Filmed at last (user installed `Xvfb` 2026-08-31; `grim` fails
  because Mutter lacks `wlr-screencopy`, and GNOME's `org.gnome.Shell.Screenshot` DBus method
  returns `AccessDenied` — portal-callers only). Recorded a cold boot on a virtual display and
  measured the window area frame-by-frame at 25fps:
  **window appears already charcoal → pure white `#fffcff` from t=2.84s to t=3.12s (~320ms) →
  charcoal + the enso splash renders correctly.**
  **So `app.windows[0].backgroundColor` WORKS** — the native window paints `#1e1e1e` from the
  first frame, which is exactly what it was added for. The white is the **WebView**, a separate
  surface: in `tauri-runtime-wry-2.10.1/src/lib.rs:920` the config colour is applied to the
  *tao window* builder only, while the webview's own `set_background_color` (`:3747`) defaults
  to **`(255,255,255,255)`** when unset. The config doc-comment claims "window and webview", but
  on GTK only the window layer is wired from config at build time. **This is the identical bug
  Android had**, fixed there natively in `MainActivity.onWebViewCreate` — two surfaces, and only
  one was covered.
  **Likely fix:** call `set_background_color` on the `WebviewWindow` in `lib.rs` setup (desktop
  cfg), so the webview surface matches before first paint. Would make `#1e1e1e` a FIFTH place
  that must stay in sync — better to derive all of them from one constant while touching this.
  **Caveat before building:** this was filmed under Xvfb **software** rendering (`libEGL
  warning: DRI3 error` in the log), so confirm the flash on the real display first — the
  compositing path may differ on a GPU. **Deferred past v1 per the settled splash scope**; the
  Android flash (the one that actually bit) is fixed. [S, deferred]
  **Trap worth keeping — `XDG_DATA_HOME` MUST be absolute.** The capture script set it from
  `SP="$(dirname "$0")"` while being invoked as `./film-splash.sh`, so `$SP` was `.` and the
  variable held a **relative** path. The XDG spec says relative values are invalid and must be
  ignored, so the app silently fell back to the real `~/.local/share/com.omni-me.app` — the run
  was NOT isolated, despite the script reading as though it were. Nothing was damaged (it read
  the DB, cleared a stale `LOCK`, touched `workspace.json`, authored no events) and it made the
  film a *warm* boot against real data rather than a fresh one, which is arguably the more
  representative test. But the failure was **silent**: no warning, no error, and the isolation
  claim looked true. Tells that caught it: a "stale SurrealKV LOCK" in a supposedly fresh dir,
  and a `device_id` older than one from an earlier run. Always pass an absolute path, and
  verify isolation by the dir's mtime rather than by reading the script.
- [ ] **The pipeline cannot tell a launchable release from an unlaunchable one.** The bug above
  survived two months because "published + correct sha + valid signature" was the whole
  acceptance test. Worth a headless smoke step in `app-release.yml` that runs the built
  AppImage under `xvfb-run` and fails the job if it exits non-zero within a few seconds. Would
  have caught this exact class on the first release. (Android can't be smoke-run in CI the same
  way — an emulator boot is a much bigger lift — so the APK stays device-verified.) [S, CI]
  **Still open 2026-09-05** — no `xvfb`/smoke step exists in either `ci.yml` or the
  overlay's `app-release.yml`.

### Deferred, with a design call attached
- [ ] The objection is **open-ended gate config**, not automation and not email as a source. Any replacement has to make "which mail is relevant" self-maintaining. Directions worth *only* a note until v1 ships — do not design these now: forward-to-a-dedicated-address push instead of polling (the user's filter action becomes the signal, no label to maintain); Gmail API query search instead of a maintained label; or drop email entirely and add API sources. Whatever replaces it inherits the constraint at `receipts.rs:216-231` — untrusted sender input reaching an LLM, with the pending-review queue as the only control. [→ own planning session, [[feedback-defer-major-phases-to-fresh-session]]]

- [ ] **Journal line timestamps — redesign, design-first** (user, 2026-07-05). Its own
  session. ⚠️ **The body of this item was lost** — in the pre-reconciliation `tasks.md` this
  was a bare heading with unrelated items filed beneath it, the same corruption that left an
  orphaned paragraph in the friction log. Only the title and the design-first framing
  survive; the requirements need re-eliciting from the user before anything is built. [M]
- [ ] **Journal template hardcodes the user's personal journaling framework** — `frontend/src/journal_template.rs::render` bakes in the user's own choices: the three reflection property keys (`homework_for_life`, `grateful_for`, `learnt_today`), the `## What happened today?` section heading, and the `daily_note` tag. Those same three keys are also hardcoded in the day-complete `is_complete` check (`core/src/events/notes_projection.rs`), and the `tags: [daily_note]` inline-list form is itself a workaround for that parser — so the template, the auto-close logic, and the typed properties panel (`journal.rs::JournalPropertiesPanel`, 3 fixed reflection fields) are all coupled to this one personal schema. Generalizing (user-configurable reflection prompts + template) means reworking `is_complete` to not key off fixed names + adding a config surface for the prompt set. Also a mild personalization-in-open-core smell (personal journaling prompts sit in the public repo, though not identity/financial data). [M] — flagged by user 2026-07-06, **deferred ("resolve later")**; pairs with 5.4 typing-feel + the properties-panel work.
- [ ] **Finances section feels slow to load / unresponsive, and the overall UI/UX lacks coherence (mobile + desktop).** User (2026-07-21): "better ways to present the data and expose interfaces for me as a user to interact with it." Two intertwined threads: **(1) perf** — the finances views feel laggy on load (seed already noted: balance-cache landed 2026-07-04, but load/interaction responsiveness in the finances section specifically still feels slow — profile the real path: command latency, projection reads, frontend render/hydration, mobile vs desktop); **(2) UX/IA redesign** — the app doesn't feel like one coherent system; rethink how finance data is presented and how the user interacts with it, on both form factors. **Cross-cutting → its own planning-first session** per the defer-major-phases rule, opened with **rendered design candidates** (per the design-render-candidates habit; design for full future scope, go wide before narrowing). Do NOT start as a tail-of-session. [L, → own session] — **IN PROGRESS — Stages A/B/C landed 2026-08-10** (plan `could-you-start-reviewing-curious-dahl.md`; approved IA = **Overview · Ledger · Analyze**). **A (perf):** read-path `tracing` instrumentation (`972cdfb`); measured real data (10,209 txns) → naive indexes insufficient (SurrealDB 3.0.4 won't skip the ORDER BY sort), so the win is frontend caching, done in C3. **B (design foundation):** CSS-var token layer + shared primitives (`Card`/`Button`/`PageHeader`/`Banner`/`StatTile`/`SegmentedNav`/`TextInput`/`Icon`) (`39a6021`); user picked Overview look **C · Balanced**. **C (IA build, 6 commits `dcd37d0`→`e87a5fb`):** C1 real net-worth-history backend (`core::dashboard::net_worth_series`, endpoint == hero; 3 core tests); C2 persistent sub-nav replacing the flat 18-variant hub (all flows preserved, surface persisted in `NavState`); C3 stale-while-revalidate frontend read-cache + skeletons (the top felt-latency lever); C4 the C·Balanced Overview (net-worth hero + range-switchable SVG area chart 1M/3M/6M/1Y/YTD/All + 2×2 card grid); C5 Ledger master-detail (desktop side-by-side / mobile slide-over, row highlight); C6 Analyze landing (cash-flow trend + budgets snapshot + reserved LLM entry). Review gate: core tests + both wasm clippy configs green, Playwright-verified 390+1280 with 0 console errors, inline-edit mutation confirmed. **REMAINING:** ~~Stage D~~ **DONE 2026-08-24** (full primitive refactor across all 5 pages + input-class fold; see #594 in the roadmap above for commit list). Still: on-device/real-data end-to-end pass (mock can't exercise the backend or real perf; rides the queued DB reset). [L, → own session]

---

## Carried backlog

**Phase-5 reconciliation / import deferrals (from Cycle 3):**
- [ ] Inline-edit per detected recurring pattern before confirm (today: dismiss + rescan). [S]
- [ ] Balancing-posting affordance for hidden-fee resolution on merge (wire/FX fees). [S]
- [ ] Credit-card CSV variant + real-export format verification (synthetic-tested only). [S]
- [ ] Reconciliation candidate engine: FX-spanning (cross-currency) matches. [M]

**Deferred stretch (from Cycle 2/3):**
- [ ] Daily Flow consistency visualizer redesign — frequency-aware (was 7-day hard-coded). [M]
- [ ] `BufferEvent::FlushFailed` → `StatusReporter` "stuck buffer" indicator. [S]
- [ ] Configurable `FORCE_GENERIC_DIRS` (hardcoded to `Work/`). [S]
- [ ] `auto_close_scheduler::AppState.event_store` → `Arc<dyn EventStore>` parity. [XS]
- [ ] Seconds duration unit on routine items (breaking event-schema change, 16 touch points). [M]
- [ ] `cargo:rerun-if-env-changed=TAURI_DEV_HOST` upstream contribution to `tauri-build`. [XS]

**Deferred from Cycle 5:**
- [ ] Create mdbooks docs, copy the pattern from ../mylearnbase in how it is created and deployed
- [ ] There will be a lot of features added to omni-me, not everyone might want to use every feature, add ability to toggle them off so there is no trace of them
- [ ] The next major thing would be adding chat functionality, so I can chat with an LLM and it can execute commands to do things instead of me needing to go find a way to do it myself

**Post-v1 / when-demanded:**
- [ ] PWA fallback (deferred Cycles 1-3).
- [ ] Veryfi `DocumentExtractor` impl (trait + routing scaffold already in place).
- [ ] ExchangeRate-API auto-rates for the manual-FX currency (replaces manual per-statement entry).
- [ ] LLM-translated NL queries for R2 (evaluate; ship only if real usage demands).
- [ ] PaddleOCR sidecar (escape hatch from Cycle-3 7.11).
- [ ] C1 email auto-fetch (vs paste); R3 self-employment dashboards; R4 tax-form validation.
- [ ] Generic IMAP config source — wire the existing public `ImapSource` into the config builder. Indefinitely deferred 2026-06-20 (needs `build_one` to thread `db`+`extractor`+async into both call sites, *and* a handler-policy design call — a config IMAP source = receipt importer by sender-pattern?). Not personally needed: the user's email sources (statements + receipts) run through the private overlay's `build_imap_sources`.
- [ ] SurrealDB bump past 3.0.4 — **lockstep across both repos** (public + private overlay each pin their own lock; out-of-sync re-floats the overlay to 3.1 + `diskann`, which fails to compile on the current toolchain, rust#100013). No vector-search usage today, so no pull; revisit when vector search is wanted or the toolchain resolves #100013. Patch 3.0.x bumps are safe meanwhile. [S]

---

## Cycle 5+ filed

- Inbox management feature (user's "far future dream").
- Open Banking Canada evaluation (when bank adoption matures).

---

## Owed at cycle close

- **Curiosities→concepts pass** — deferred once already; do not skip again at cycle close.
  (The *code review* half of this item was struck 2026-09-05: the pre-v1 gate was a full
  end-to-end read-the-code pass and it is closed, which subsumes Cycle 3's code. The item had
  been double-counting a review that already happened.)
- **Memory prune pass** — `MEMORY.md` index is the retrieval surface and is near its cap.
  Content-first, per note; classifying by filename is what went wrong last attempt.
- **Session-start staleness signal for artifacts beyond `NEXT.md`** — queued by the user
  2026-09-04. The design risk is noise, not mechanism: a canary that fires most sessions
  trains you to skim past all of them, so thresholds need to differ per artifact.
  Meanwhile the `Reconciled against git` line at the top of this file is the manual stand-in.
