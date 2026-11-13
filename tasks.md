# Tasks — Cycle 5

**Reset 2026-11-12 at the Cycle 4 close-out** (pre-reset file: `git show 7ef0e55:tasks.md`).
Each open item was checked against the code before it carried. "Verified open" means the
reset read the code, and "unverified" means it did not. Rulings moved to `project.md`
§ Decisions in force.

## How to read this file

- **Open work only.** A finished item is deleted, not ticked. Git keeps the history.
- **One item, at most three lines:** what, why it matters, where. Rationale goes to
  `docs/src/` or the edit site, and rulings go to `project.md`. That routing is what keeps
  this file under its cap (400 lines / 40 KB). The last file grew to 5,800 lines because
  narrative landed here.
- **`[NNNN]`** is the item's id in `.omni/tasks.toml` (`omni-me-NNNN`). Never reuse one.
- **`NEXT.md` outranks this file** on what to do next. Priority is his to set.

Size tags: [XS] ≤30min · [S] ~1h · [M] ~2-3h · [L] ~4-6h · [USER] needs him

## Now — dated

- [0001] **11-16: the weekly WS buys arrive whole**: two legs, units to the ticker
  account, no `Unmatched`. The fix shipped 11-12 (overlay `05e67a9`, driver copied to live's
  `/opt/omni-ws`), and only a new buy proves it. [XS]
- [0002] **Release 1.2.4, after [0001], his go.** Queue: `63728f7` (tax-authority notices
  file as statements; server) and `918a9d5` (retention review shows the counted-from date;
  server + app). Also on the branch since: the job contract and Settings → Jobs (`d6ded9e`),
  ntfy alerts and quiet hours (`a9cd960`), the demo build (`dd0bc0c`), the daily summary. The
  server now folds the assistant projection: one ~4 min replay at that deploy, inside the gate.
  Add each new fix here. A fast-forward of public `main` closes the draft PR
  (#8), so open a new one after. [M, USER]

## Cycle 5 sequence (design session, 2026-11-12; rulings in `project.md` § Notifications, § Agent framework)

- [0079] **Slice 1's live half, his steps:** the code is on the dev branch. Bring ntfy up and
  make users and the server token (overlay `deploy/ntfy/README.md`, files already in
  `~/omni-ntfy`), then the phone app. Live alerts start with 1.2.4 and its `.env` lines. [S, USER]
- [0085] **Logbook: the assistant asks before it acts.** Scaffold in `logbook/_drafts/` (device-
  local, gitignored). Demo: push `demo/assistant`; [0084] held at 1.5 MB brotli. Jan deadline. [M]
- [0086] **Logbook: receipts from email.** Scaffold in `logbook/_drafts/`. Evidence: a Settings →
  Jobs screenshot from live after 1.2.4, and the C1/C0 bench scores. [M]
- [0087] **Logbook: search by meaning.** Scaffold drafted. The mock bridge returns canned search
  results, so the demo needs a decision: port fusion to the browser, or lead with the bench. [M]
- [0088] **Logbook: the document archive.** Scaffold drafted. Demo: push `demo/archive`. [M]
- [0081] **Slice 3:** measure critical's weekly budget; over it, the summary asks to demote. [S]
- [0082] **Slice 4, each ruled on its own:** large charge (threshold setting), new merchant
  (on/off), penalty-dated bills (no due date is modelled yet). Statement mismatch is [0050]. [L]

## Waiting on his decision

- [0003] **N-to-1 receipt matching: build it or not.** `find_match_candidates` pairs one
  receipt with one same-amount charge, so a booking billed as several charges (the flights)
  always needs a hand merge. [M if built]
- [0004] **Two pre-November drafts in live review:** a 10-30 rent e-transfer (795.00) and a
  10-31 grocery "add more items" mail. Dismiss or keep. [USER]
- [0006] **R2 for the off-box blob copy:** his Cloudflare account, a bucket and a scoped
  API token. I build the backup job after that. Blobs have exactly one copy today. [USER, then S]
- [0007] **GitHub Support purge of the pre-S-W1 commits** (optional). GitHub still serves
  them by SHA. [USER]
- [0008] **F3's open half:** should multi-page reading be told that pages can repeat? A
  two-page receipt double-counted, and the cross-check caught it. [XS]
- [0009] **Tap targets:** a visual redesign across screens. The worst are Settings' feature
  toggles at 358×20 and Journal's "Raw properties" at 104×16. [M, USER]
- [0010] **R27's account side:** OpenRouter's data-collection setting and the upstreams'
  retention. [USER]
- [0011] **Tab order without a recompile: global or per device?** Decide the landing tab's
  own key at the same time, because `home_tab()` is the first visible tab. A tab missing from
  a stored order is appended. [M]
- [0012] **Crypto is modelled monthly** (60 of 62 daily rows against 4 aggregates; balances
  exact). Was that intended? [XS]
- [0013] **When the authoritative receipt fails to price**, should review offer the
  confirmation's estimate as a fallback? Ask before building. [S]
- [0014] **Should a persisted `server_url` outrank a compile-time default** that changed
  under it? That precedence is what made a reinstalled desktop unreachable. [S]
- [0015] **Journal line timestamps redesign.** The requirements were lost in an old
  corruption of this file. Re-elicit them before designing anything. [M]
- [0016] **Is import parity with the paisa importers still wanted?** Live now imports
  through paisa, then the feeds. Four of seven were covered in-app. [M if yes]

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

- [0027] **Frontend clippy lacks `--all-targets`** (verified, `ci.yml` wasm jobs). Adding it
  trips a deliberate `expect(dead_code)` in `diagnostics.rs`. Raise that, never flip it. [XS]
- [0028] **An oversized pushed event retries forever** (verified). The server answers 413,
  and `sync/client.rs` treats only 400 as `Rejected`. Latent: real payloads are ~3 KB. [S]
- [0029] **A zero-amount receipt draft got past the "no non-zero amount" check**: a single
  `Unmatched -0` posting, live 11-10. Cause unverified. [S]
- [0030] **Citation chips show the record's kind, not its title** (verified). `EvidenceRef
  .title` is `None` off the wire by design, and nothing resolves it locally. [S]
- [0031] **The Ledger restores pre-wipe rows until Apply** (his nitpick, 11-10). A restored
  list skips its first fetch, and a boot rebuild emits no `sync:applied`. [S]
- [0032] **The desktop's post-update splash hang is back, intermittently** (his nitpick, 11-10;
  11-11 was clean). The desktop's log from that launch would settle it. [S–M]
- [0033] **Mail attachments reach the vision model unrotated.** Captures are fixed. A
  mail photo's EXIF orientation is ignored by `prepare_image`. [S]
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
- [0039] **Small:** `archive::ingest_paths`/`walk_dir` are dead code; the photo picker keeps
  showing "2 files"; a notes-search problem report lacks the query and result count. [XS each]

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
- [0047] **Say "4 items listed, the total covers more"** on a receipt whose items fall short
  of its total. The money side is already right. [S]
- [0048] **The retention scan is capped at 500 documents per open and runs on the device.**
  [S]
- [0049] **Split a document across requests past an endpoint's image cap.** This is low:
  model choice avoids it, and `MAX_DOCUMENT_PARTS = 8` refuses 2.6% regardless. [S]
- [0050] **Statements as the source-of-truth health check:** assert the feeds against the
  closing balances. [L]
- [0051] **A categorization classifier.** It is gated on posting provenance, and it is
  scored on abstention. [L]
- [0052] **Reconciliation deferrals:** inline-edit a detected recurring pattern; a balancing
  posting for hidden fees; a credit-card CSV variant; FX-spanning matches. Optionally, Resolve
  could suggest nothing for a description that names no payer, rather than inheriting the
  majority (a cheque became `Income:Business`). [S–M each]

**Finances**
- [0053] **Privacy eye: hide every amount with one tap.** A mask that misses one number is
  worse than none. Route every money render through one formatter. [M]
- [0054] **Swipe between Overview, Ledger and Analyze** (two swipe handlers already exist). [S]
- [0055] **Finances performance on real data** (11k transactions on live). Observe; do not
  pre-optimise. [—]

**Generalization, trailing** (`docs/src/invariants.md` is the contract)
- [0056] **Preset picker UI, extra presets, record-type export and import, setup guides.**
  The small constants now have a home (`FORCE_GENERIC_DIRS`, vault naming). [M–L]
- [0057] **Publish the mdbook.** Pages has never been enabled. Turn it on, then set
  `vars.PUBLISH_DOCS`. Held until there is a setup guide to follow. [XS, USER]

**Platform, CI and ops**
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

## Owed at cycle close

- [0071] **Staleness signals for artifacts beyond `NEXT.md`.** The `tasks.md` size cap
  (dotfiles `944036d`) is the first one. Too
  many canaries and they get skimmed, so thresholds differ per artifact. [S]
- [0072] **Scrub the private identity from this PUBLIC repo** (authorised; at the end of an
  autonomous session, never first). Known sites: `docs/src/auto-import.md:6` and the doc
  comments and fixtures in `core/src/auto_import/receipts.rs`. Sweep `docs/` and `.archive/`.
  Rewrite each name to the role it plays, and check every fixture's assertion too. Sized
  2026-11-13: ~90 hits in 28 files (merchants, the city, its utility), plus the mock image
  `mocks/receipt-loblaws.png`; the denylist catches none of them. Its own pass. [L]
- [0073] **Revise the comment convention** (own session, design-first), then the sweep it
  gates. The routing is right; the length and voice failed. [L]

## Later cycles

- [0091] **Recurring detection to the server**, when it migrates onto the contract: it needs no
  offline run, and every open device runs it today (a duplicate risk, unverified). [M]
- [0074] **Task and project records** (sequence item 4). These gate estimation calibration,
  and the assistant gains them with no assistant code. [L]
- Filed, not scheduled: inbox management · Open Banking Canada · Gmail write-back (once trust
  is built) · blobs on object storage and the agent on its own machine (these must not
  complicate the co-located design).
