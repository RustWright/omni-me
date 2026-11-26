# Tasks — Cycle 6

**Reset 2026-11-19 at Cycle 6 Planning** (pre-reset file: `git show 16f67f6:tasks.md`). The
Cycle 5 items carried as curated at its 11-18 close, with their labels: "verified open" means a
reset read the code, and "unverified" means it did not. Rulings live in `RULINGS.md`.

## How to read this file

- **Open work only.** A finished item is deleted, not ticked. Git keeps the history.
- **One item, at most three lines:** what, why it matters, where. Rationale goes to
  `docs/src/` or the edit site, and rulings go to `RULINGS.md`. That routing is what keeps
  this file under its cap (400 lines / 40 KB). The last file grew to 5,800 lines because
  narrative landed here.
- **`[NNNN]`** is the item's id in `.omni/tasks.toml` (`omni-me-NNNN`). Never reuse one.
- **`NEXT.md` outranks this file** on what to do next. Priority is his to set.

Size tags: [XS] ≤30min · [S] ~1h · [M] ~2-3h · [L] ~4-6h · [USER] needs him

## Cycle 6: [0074] tasks, planning, people

Rulings 90–131, the UI ruled in `design/0102/`, the preps in overlay `*_PREP.md`;
evidence in `PUSH_0074_RESEARCH.md`. In dependency order. Prep and design sessions need him;
builds follow them.
- [0115] **Held until [0109] lands** (his call, 11-22: planning may revise task decisions). **Task inputs only he can give:** go for the seed import (overlay `TASKS_PREP.md`); wiring
  `scripts/task-hub.sh` into the shared session hooks (push and SessionStart receipt, 91, 121);
  the LEARN feed address as `learn_feed_url` in dev and live secrets (122). [USER]
- [0116] **Task surfaces, with [0109]:** unattached deadlines and `USER` hub items reach him in a
  ritual, the hub trial's acted/dismissed record (92), courses closed at term end (123), staleness
  thresholds as settings (92), timer controls on the view (93), an "inferred" mark on ticks (127). [M]
- [0109] **Planning build:** read-only calendar (96; source interface public, Google connector
  in the overlay, 133), plan records, the ritual tier and summary
  lines (98), block reminders, the load measure (99). Needs [0106]. Reminders per [0104]'s results
  (`PUSH_0074_RESEARCH.md` Appendix B): plugin 2.4.0, reschedule on every start, `USE_EXACT_ALARM`
  (his phone is Android 15), `start:<block>` action ids; MainActivity reads a Start tap
  from the launch intent so it works on a closed app (his call), handed over like the deep link. [L]
  Built as the slices below, in dependency order; [0109] closes when they all have.
- [0123] **Plans and the planning specialist: built** (`e2cf053`, `docs/src/planning.md`). Left:
  his places seeded on live (126) on his go, by the `/planning/places` calls in that doc; on live
  also set Settings → Planning (bedtime, morning and evening routine names). [XS]
- [0124] **Rituals, summary lines, load, goals: built** (`456814a`..`58ee6f1`, `docs/src/planning.md`
  § The other rituals, § Goals; goals are ruling 134). Real path ran on dev 11-26 (hour set to 0):
  every scheduled question was routed, on the interactive seat, because `pending_questions`
  dropped `prompt_name`; monthly went to General and said it had no `goal.set`. Fixed in the
  commit after `c3d67c8`. Left: rerun on an agent image with the fix (evening any day, weekly
  Sunday, monthly the 1st), then his read of the prompt texts in `planning/rituals.rs`. [S]
- [0126] **Plan card and view: built**, row taps open chat (`docs/src/planning.md` § The card and
  the view). Left: the taps seen on the S9 with the next dev APK; clippy-clean, never run. [XS]
- [0127] **Google connector (overlay, 133):** read-only OAuth, window snapshots, both calendars. Needs
  his Google Cloud setup, walked through live when this slice starts. Also wires the server's
  15-minute read (113) through `RunConfig`, since the public server has no source, and the fresh
  read before each plan (113): until then the brief and `propose` read the stored calendar. A
  failed read should leave a record, so the plan card can say "Calendar: couldn't refresh" (117). [M]
- [0128] **Live may OOM-loop on its next restart:** SurrealKV's startup compaction needed ~2.1 GB
  on dev's 2.2 GB volume against a 1500m cap and looped forever (dev, 11-25; fixed by compacting a
  copy off-box, `~/omni-dev-tools/state/dev-server-0128/`, old data kept as `_data.pre-0128`).
  Live has the same cap, a 1.9 GB volume, and no restart since 11-22. Discussed at the live
  release (his call): planned off-box compaction (~1.5 h downtime), a bigger box, or leave it. Never test on the box
  (`feedback-no-memory-experiments-beside-live`). [S]
- [0110] **People build:** mentions, facts, resolution, pickers and group cards (102–104), merge
  and unmerge reversible for every attached record from the first event, the person view and the
  People tab with model-free search (118–120), per `design/0102/`; task text as a source (124);
  the fact shape (128), recall first (129). Seed and evidence: overlay `PEOPLE_PREP.md`. [L]

## Waiting on his decision

- [0013] (Parked 11-16 until a real blank delivery item appears; ask with it on screen.)
  **When the authoritative receipt fails to price**, should review offer the
  confirmation's estimate as a fallback? Ask before building. [S]

## Owed to him

- [0017] **The monthly LLM spend.** No figure exists: the provider reports `estimated_cost`
  per call and nothing reads it (`MODEL_BENCH.md` R4b). His billing page settles it today. [S]

## Design calls — mine to propose before building

- [0083] **Path-scoped rules kept out of the public repo** (they never load via a symlink). A,
  my pick: `~/.claude/rules/omni-me/`, if a test shows user rules load from subfolders. B:
  gitignored `.claude/rules/`, hook-synced both ways. Test at implementation; then move traps. [M]

- [0019] **`check_total` holds for one hint in four.** Paystub is penalised whatever the
  model emits (`verify.rs`). The fix is sign-aware per hint, or a redefined `total`. Check
  what the paystub sign convention carries first. [M]
- [0020] **The non-production banner cannot fire on Android** (verified open). The posture
  comes only from the `OMNI_DATA_DIR` env var, so a dev APK looks exactly like live. The
  options are a build-time flag or the server's `/health` instance. [S–M]
- [0021] **The server authors events with no feature gate** (verified open). It has no
  `ResolvedConfig` (`routes/documents.rs` says so), so switching a feature off stops nothing
  server-side. Fix it whole, not piecemeal. [M]
- [0022] **`pull_only` holds up to 100k pulled events in one Vec** (verified open,
  `sync/client.rs`). The fold needs `replay_paged` with a progress callback first, then three
  callers move. `init_all` is silent for minutes on a fresh install too. [M]
- [0023] **Last-write-wins guards degrade to "always apply" on a failed read**
  (`config_projection.rs:100`, `record_type_projection.rs:91`, `.unwrap_or(None)`). This is a
  semantics call. [S]
- [0024] **Field plausibility:** is `commodity` a currency code? Every guard inspects the
  numbers, and none checks that a field means what it claims. [S]
- [0025] **Stemming needs a migration:** `omni_text` has no stemmer ("running" misses "run").
  Adding one means an index change and a projection bump. [M]
- [0026] **`budget.set` needs a catalogued `budget` type**, which means a `category` column
  and a `BudgetProjection` bump. Without the type, the action is undiscoverable. [M]

## Bugs

- [0119] **A currency conversion arrived as two one-sided rows** (his report, Nov 22 USD to CAD): the
  money-transfer source reports each side per balance under one reference. Fixed: joined after dedup, target leg priced
  (overlay `a9d79e4`); next release. The Nov 22 one was merged on live by script. [XS]
- [0120] **Two-currency entries are priced on save** (ruling 132): built, CI green, verified on the
  S9 (preview line, saved rate, re-price after an edit). Ships with the next release. [S]
- [0118] **Watch the brokerage transfer fix on live:** shipped in v1.2.8; confirm no lone
  transfer row reaches Reconcile when the next joined transfer's busy leg ages out. [XS]
- [0097] **A photo-only email fails the receipt handler** ("produced no extractable text": it
  reads the body and PDF text only), though the archive still files and reads the photo. Whether
  a receipt draft results is unverified (his test mail, dev uid 15062, 2026-11-16). [S]
- [0100] **A revoked IMAP password never becomes "needs you"** (review T1, verified): a login
  failure maps to `Upstream`, so a plain Failure, and only the 24 h "failing" alert fires. Design
  first: `NeedsReauth` halts the source and only the OTP Reconnect flow resumes it. [M]
- [0029] **A zero-amount receipt draft got past the "no non-zero amount" check**: a single
  `Unmatched -0` posting, live 11-10. Cause unverified. [S]
- [0032] **The desktop's post-update splash hang is back, intermittently** (11-10 hung, 11-11
  clean, 11-16 hung on 1.2.3→1.2.4; a restart clears it). The app logs to stderr only, so an
  updater launch leaves no log: capture needs a file writer or a terminal launch. [S–M]
- [0034] **Unverified oddities:** `list_documents` with `mime: "image/jpeg"` returned nothing
  (10-01; the query looks right); the yahoo backfill lists uids descending against
  `docs/src/archive.md`; and whether the cursor-reset route takes a `-backfill` source. [XS]
- [0035] **The box's GHCR token is hand-placed and expires silently every 90 days.** It
  breaks live deploys. `deploy.yml` should log the box in from the Actions secret instead.
  Current expiry unknown. [S]
- [0036] **The CodeMirror cursor lands behind the soft keyboard and the drawer.** Plain
  inputs are fixed (`keyboard.rs` skips CodeMirror). Nothing listens to `visualViewport`. [M]
- [0037] **Sync feels laggy, and an open view sometimes stays stale.** There is a 20 s poll
  and no push channel. The suspected stale paths are the `sync:applied` nudge and editor
  dirty-protect. [M]
- [0038] **Wipe snapshots are never pruned, and a restore has never been tried.** [S]

## From daily use — backlog

**Assistant**
- [0040] **The `search` verb takes no filters** (verified), so a tag cannot combine with a text
  search. Only `list` filters. [S]
- [0041] **No weekday frequency for routines** (verified: Daily, Weekly, Biweekly, Monthly,
  Custom). "Weekday mornings" cannot be expressed. [M]
- [0042] **Ground `categorize`/`tag` in categories already in use**, as `record` already is. [S]
- [0043] **A per-role turn budget.** `assistant.max_turns` is global. This is the
  prerequisite for beliefs' preferred shape, and it keeps a floor. [M]
- [0044] **A second scheduled question for finding beliefs.** The review prompt says "do not
  draw new conclusions", so this cannot be bolted onto it. Blocked on [0043]. [S]
- [0045] **Real step-by-step progress in chat** (ruled for after the merge). The agent
  publishes its current step as a short-lived server status. [M]
- [0046] **Role D's prompt:** it passes no entry date for relative dates, gives no
  permission to leave a category empty, and has no untrusted-input warning. Deferred until
  after the slate, and the slate has run. [S]
- [0075] **Beliefs have no filter for retired ones.** Retirement is `superseded_at` being set,
  and the catalogue offers no filter over it, so "my current beliefs" cannot be listed. [S]

**Archive and import**
- [0008] **Tell multi-page reading that pages can repeat** (ruled 11-16): a later page may
  repeat lines, count each once, a page showing the charged amount gives the total. Guard with
  two cases: the 10-02 repeating receipt and the `extraction/mod.rs:1281` tip slip. [XS]
- [0047] **Say "4 items listed, the total covers more"** on a receipt whose items fall short
  of its total. The money side is already right. [S]
- [0048] **The retention scan is capped at 500 documents per open and runs on the device.**
  [S]
- [0049] **Split a document across requests past an endpoint's image cap.** This is low:
  model choice avoids it, and `MAX_DOCUMENT_PARTS = 8` refuses 2.6% regardless. [S]
- [0050] **Statements as the source-of-truth health check:** assert the feeds against the
  closing balances. Absorbs [0016] (cut 11-16): read a source's closing balance only when the
  check needs it; four of seven formats already parse. [L]
- [0051] **A categorization classifier.** It is gated on posting provenance, and it is
  scored on abstention. [L]
- [0052] **Reconciliation deferrals:** inline-edit a detected recurring pattern; a balancing
  posting for hidden fees; a credit-card CSV variant; FX-spanning matches. Optionally, Resolve
  could suggest nothing for a description that names no payer, rather than inheriting the
  majority (a cheque became `Income:Business`). [S–M each]

**Finances**
- [0054] **Swipe between Overview, Ledger and Analyze** (two swipe handlers already exist). [S]
- [0055] **Finances performance on real data** (11k transactions on live). Observe; do not
  pre-optimise. Watch: citation titles filter every row per chip set (P-I1: ~11 ms vs ~2.5 on
  the dev S9); fix is a by-id lookup, batched per thread. [—]

**Generalization, trailing** (`docs/src/invariants.md` is the contract)
- [0056] **Preset picker UI, extra presets, record-type export and import, setup guides.**
  The small constants now have a home (`FORCE_GENERIC_DIRS`, vault naming). [M–L]
- [0011] **Tab order without a recompile** (ruled 11-16: per device, in the device config
  layer). The app opens on the first visible tab of that order, with no separate key. A tab
  missing from a stored order is appended in default order. Not urgent. [M]
- [0057] **Publish the mdbook.** Pages has never been enabled. Turn it on, then set
  `vars.PUBLISH_DOCS`. Held until there is a setup guide to follow. [XS, USER]

**Platform, CI and ops**
- [0014] **Persist `server_url` only when he sets it in Settings** (ruled 11-16). The first run
  writes the resolved default today (`runtime.rs` `resolve`), so a stale default outranks every
  later build's. Plus a one-time clean-up of auto-written files. [S]
- [0058] **A headless smoke step for the AppImage** in `app-release.yml` (verified missing). [S]
- [0059] **Docker builds recompile every dependency** (verified). Cache mounts are not
  exported by `type=gha`. The fix is `buildkit-cache-dance`, and it is ~18 min per build. [S]
- [0060] **The desktop flashes white for ~320 ms on boot.** It is the webview surface, not
  the window. This was deferred past v1. [S]
- [0061] **A growth policy for the dev instance** (retention or re-clone). There is no swap,
  so pressure means an OOM kill. [S]
- [0062] **The wipe runbook omits client-side credential strays**: `$XDG_CONFIG_HOME/omni-me/`
  and an old `ws-session.json` on any machine that ran a prototype. [XS]
- [0063] **Feedback's last device leg: a real wasm panic** flushing to `localStorage` and
  restoring as `[prev]`. [S]
- [0064] **Older flags, still small:** the `AppState.event_store` → `Arc<dyn EventStore>`
  parity; a seconds unit on routine items (a breaking schema change); a `FlushFailed` →
  "stuck buffer" indicator; the frequency-aware Daily Flow visualizer; the upstream
  `rerun-if-env-changed` for `tauri-build`. [XS–M]

- [0099] **`jobs/` consolidation** (Cycle 5 review, deferred 11-17): the summary walks the badge
  queues with copied SQL (B-W3); `charge_of` hand-parses typed drafts, so decode leniently or lose
  alerts (B-W4); one `AlertTier` enum (B-I4); a shared test kit (B-I5). [M]

**Benches and models** (`MODEL_BENCH.md`)
- [0065] **Extend the retrieval bench past `search` over notes and journal.** The
  `document` type matters most and has never been retrieval-tested. [M]
- [0066] **Research what would move seat A** (speculative decoding, a warm tier, the tails).
  Prefix caching is already in (`b4a394a`). [M]
- [0067] **Widen the C2 probe set.** One abstention probe carries the whole first key. Also
  print fabrications per successful run. [M]
- [0068] **Compare role E's four embedders** (one arm today), and **GLM-5.3 for the batch
  seat** via `--bench-review`. [S spend each]
- [0069] **Vision parity per role is unproven.** There are no photos on this laptop, and
  rendered PDF pages are unbuilt as a source. [M]

**When demanded** (post-v1): PWA fallback · a Veryfi extractor · auto FX rates · NL queries ·
a PaddleOCR sidecar · email auto-fetch, self-employment dashboards, tax-form validation · a
generic IMAP config source.

## Alongside Cycle 6

- [0111] **Standing test debt** (Cycle 5 review): T5, T8, T9, T14, T15, T16, T18, T19, each
  written when its code is next edited. T5 (stored rows after a type change) also gates areas. [M]

## Later cycles

- [0113] **Locations** (`research.md` § Locations, ruling 115): location history as event
  metadata, reusing planning's named places and feeding them real travel times. Its own cycle:
  background location, battery, and the most sensitive data the app would sync. [L]

- [0091] **Recurring detection to the server**, when it migrates onto the contract: it needs no
  offline run, and every open device runs it today (a duplicate risk, unverified). [M]
- [0003] **One receipt to several charges** (shelved 11-16). Trigger: more than 5 in one week.
  App merge refuses legs that do not cancel (`plan_merge`), so there is no in-app path today;
  the hand workaround (edit one charge to the sum, delete the other) is unverified. [M]
- [0009] **Tap targets** (shelved 11-16: no mis-taps in real use). Trigger: he reports one, or
  the LLM-primary push redesigns the surviving screens. A global min-hit-area floor first. [S–M]
- [0098] **omni-me's own UnifiedPush channel in place of ntfy** (his direction 11-17, after the
  more important work). `notify.rs`'s `Notifier` trait is the seam. Until then, `omni-me://` links
  stay tab-only: BROWSABLE lets any page fire them (S-I1). [L]
- Filed, not scheduled: inbox management · Open Banking Canada · Gmail write-back (once trust
  is built) · blobs on object storage and the agent on its own machine (these must not
  complicate the co-located design).
