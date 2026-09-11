# Tasks — post-v1 daily use

**Status:** v1.1.0, released 2026-09-06 and published to `/updates`; both devices still
run v1.0.5 until the OTA is taken, and **on-device verification is open** (see `NEXT.md`).
The phone and `surface` (the Linux desktop) sync against the Hetzner box.

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
2. **Generalization** — **STRUCTURAL PHASES COMPLETE 2026-09-06** (0, A, B, C all built and
   verified). The narrow framing (three hardcoded strings) was replaced by the real question:
   omni-me is meant to become a system other people run and customize, without being forced
   into the user's own preferences. What remains is trailing, not structural: a preset picker
   UI, extra presets, record-type export/import.
   12 decisions settled; full reasoning in `~/.claude/plans/lets-continue-deep-blanket.md`,
   the second-user-facing half published in `docs/src/invariants.md`. Phases below.
   ⚠️ **Statement layout strings are OUT of scope** — item 2 named them, but the finance
   block below forbids finance work and it controls.
3. **AI / LLM / ML integration — PLANNED 2026-09-07, not built.** Design in
   `~/.claude/plans/lets-plan-how-to-sprightly-hartmanis.md`; build sequence Phase 0 → G below.
   ⚠️ **This item's original framing is superseded.** It said "re-examine
   `DocumentExtractor`/Gemini"; the user reframed it into **a personal assistant with reach
   across the whole app** — the more of his life lands in omni-me, the more the model can help
   him stay organised. And the premise was wrong: an inventory found the existing LLM surface
   has **no reachable consumer at all** (the text path lost its UI trigger 2026-07-04;
   extraction's only call sites are behind ⛔-deferred finances), so there was nothing to
   re-evaluate. Nothing in it is load-bearing; it is not to be preserved.
   - **Interface:** a fixed verb set (`search`/`read`/`propose`/`list_types`/`describe_type`)
     generic over declared record types, because tool accuracy degrades past ~15–20 tools.
     New features add data, never tools.
   - **Agent shape:** a headless omni-me device — own process, own DB, own device id, syncing
     like the phone. Forced by SurrealKV being per-process.
   - **Autonomy:** proposal-only, earned per action type, destination "reversible acts freely".
   - **Hosting — DECIDED 2026-09-08: DeepInfra, open weights only.** Screened the 41
     zero-retention providers on OpenRouter against the six criteria; the EU-jurisdiction
     criterion turned out to answer state access rather than the commercial-misuse threat
     actually held, and the CLOUD Act follows the provider anyway, so it was demoted to a
     tiebreak. **Accepted risk, explicit: US jurisdiction, no recourse against US legal
     process.** DeepInfra owns no models, ZDR by default, 99.982% SLA, own hardware, ~4×
     cheaper and more reliable than the EU alternative. Rationale in `docs/src/assistant.md`;
     full comparison in `~/.claude/plans/so-i-made-a-mighty-boot.md`.
     - **Bench = production, structurally.** Screening runs on OpenRouter *pinned to
       DeepInfra tags* (the script refuses any other pin without an explicit override); the
       deciding run goes direct. A benchmark can no longer validate a stack we cannot ship.
     - **Gemini is gone** — client, extractor, `[gemini]` credentials, the fallback. A
       closed model from a model owner is what criterion 1 excludes. `build_llm_client` now
       refuses closed-weights ids (DeepInfra's catalogue proxies Anthropic/Google, where its
       retention policy does not apply) unless `allow_closed_weights` opts in.
     - ⚠️ **`poppler-utils` was missing from the production image**, so `pdftotext` ENOENT'd
       and PDF extraction had never worked on the box. Added. PDFs now convert to text
       before the model call — generated PDFs only; scanned ones need rasterization, unbuilt.
     - No hardware purchase — the user has no permanent address.
   - ~~⚠️ **Blocker for any write path:** `guard_event_type` lives in the Tauri commands layer~~
     **RESOLVED in Phase A.** The guard moved into `core` as `EventWriter`
     (`core/src/events/writer.rs`), which welds guard + append + project + push-nudge into one
     type seated on `ResolvedConfig`. It is now the only sanctioned way to author an event, and
     the agent inherits it rather than re-implementing it.
   - **Phase 0 done 2026-09-07:** `docs/src/assistant.md` (the contract, model-neutral) and the
     RAM spike (**no CX32** — CX22 has 3.1 GiB available, server + SurrealKV = 106 MiB against a
     52 MB DB; the real risks are no swap, no container memory limits, and Phase C's embedder at
     0.1–1 GB). Remaining: the model bake-off.
   - **Phase A done + verified end-to-end 2026-09-08.** The agent is a real headless device:
     own process, own DB, own device id, syncing over HTTP. `EventWriter` is the single append
     boundary; `registry::build_projections_headless` picks projections and excludes
     `JournalFile` structurally; cold start pulls config *before* choosing projections, because
     an empty DB resolves every feature to its default of `on` and for an agent a cold start is
     the normal case. Registers no verbs and calls no model — that is Phase B.
     - **Verified against a throwaway hub**, all four checks: cold-start backfill; an authored
       event reaching a second agent **carrying the first's device id**; authoring refused with
       the feature off; that same event still applied when arriving *inbound*. Recipe in memory
       (`reference-throwaway-hub-agent-testing`) — no docker, no sudo.
     - **`--probe` is a permanent ops flag** (user, 2026-09-08), the twin of `--read-only`: one
       refuses to build a writer, the other authors one throwaway note through the one it built.
       It writes a real note, so it is for throwaway hubs only.
     - ⚠️ **The verification found a silent, permanent projection bug** — `init_all` seeded a
       first-sight watermark to `time::now()`, which lands *after* events already in the store,
       so the agent's cold-start pull was never projected and it booted on default features
       regardless of config. Fixed by distinguishing "no row" (never ran → seed epoch) from
       "row present, field NONE" (upgrade → seed now), with a regression test. Still latent for
       any *newly-shipped* projection on an existing install, which this now also covers.
   - **Phase B built 2026-09-08 — the four read verbs, live end-to-end.** `search` / `read` /
     `list_types` / `describe_type`, a multi-turn loop, and cost instrumentation from the first
     call. Free-form verb selection scored **10/10** against the free endpoint on a seeded
     throwaway hub; a real question ran `list_types → search → read ×2 → answer` in 5 turns and
     cited both a journal entry and a note correctly.
     - ⚠️ **`RecordType` was NOT extended, and that is the phase's main decision.** It is a
       *document form* schema (generalization decisions 2 + 9 — "forms rendered from record
       types"); its properties are frontmatter keys and it cannot describe a routine. The
       assistant reads a separate **read catalog** (`core/src/assistant/catalog.rs`), which is
       **code rather than events** because every table it names is created by a projection in
       code — a declaration pointing at a missing table would be genericity in name only. The
       two compose: `describe_type` reads the live `RecordType` where a type has one.
       Journal, note and routine are registered; a **transaction entry is a test fixture that
       never registers**, so the struct was designed against a non-document shape without
       touching ⛔-deferred finance data.
     - **The five-verb set is flexible** (user, 2026-09-08). What is load-bearing is why it
       stays small, not the number. Changes get proposed and discussed, with a reason.
     - **`--ask` and `--bench` are dev tools**, unlike `--probe`. ⛔ **Superseded 2026-09-10:
       they do NOT go when the chat surface lands.** The Assistant tab shipped and both stay —
       the tab prints prose where `--ask` prints the trace, `--ask --constrained` is the
       constrained-path smoke test `bench-openrouter.sh` branches on, and model re-evaluation
       needs `--bench` on a standing basis. `--ask` is dead only *on the box*, where a resident
       agent holds the `surrealkv` lock.
     - Search is SurrealDB `FULLTEXT` + BM25 + `HIGHLIGHTS` — verified working on the embedded
       `kv-surrealkv` engine before anything depended on it, and the same `DEFINE INDEX` family
       Phase C's `HNSW` will use. The app gets its first real search as a side effect.
     - ⚠️ **Four traps found by building it, all now regression-tested:** BM25 is
       corpus-relative and legitimately scores `0.0` for a real match in a small corpus, so
       `search` must never threshold on `> 0` · `search::highlight` marks the whole field and
       its markers reached a user-facing answer on the first live run, hence keyword-in-context
       windowing · **constrained decoding suppresses tool calling rather than failing**, so the
       loop parses `{"verb":…}` out of message content or the tax measures the transport ·
       rate limiting is a *correctness* control on a capped tier, not politeness.
     - **Still owed: the constraint tax. Attempted 2026-09-08 and NOT measured — the harness
       is unsound, and that is the finding.** ⛔ `Session::ask` sends `verbs::tools()` **and**
       `response_format` together in the constrained variant, so the model is handed two ways
       to call a verb and picks one by training: `gpt-oss-120b` ignored the schema and used
       native tool calls (2/2 on DeepInfra), making its "constrained" run free-form and its
       tax ≈ 0 by construction; `glm-5.3-flash` emitted a hybrid whose `arguments` carried
       tool-call fields, recording a verb it did not want; the archived spike saw the third
       behaviour. **A number that varies by which channel a model prefers cannot be compared
       across models**, which is the whole point of measuring it.
       - **The fork before any re-run** (product decision, not a tidy-up): dropping `tools`
         when constrained is right, but the verb documentation lives in `tools()`, so that
         half becomes less *informed* and the run measures information loss instead. Proposed:
         render the same docs into the system prompt for that variant.
       - **Also blocked externally that day:** `glm-5.3-flash` was rate-limited upstream on
         **every** provider tried (DeepInfra and Fireworks, `upstream_provider_shared_pool`).
         Not our request rate, not credit. `gpt-oss-120b` stayed reachable.
     - **Built while getting there, all tested:** bench case 5 corrected to expect `list` (it
       still expected `search` after `list` shipped, scoring correct behaviour as wrong in
       both halves) · `ChatResponse.provider` parses and logs which upstream served each call,
       so a pin is evidenced rather than asserted · `--ask --constrained` as a one-request
       rehearsal · bounded 429 retry with doubling backoff, honouring `Retry-After`, because
       `allow_fallbacks:false` makes upstream congestion arrive as a hard failure ·
       `scripts/seed-bench-hub.py` and `scripts/bench-openrouter.sh`, which were previously
       ad-hoc shell recoverable only from a session log.
   - **First slate run, 2026-09-09 — 9 models, 11 runs, ~$3, all pinned to DeepInfra.**
     Scorecards in `runs/20260909-093234/` (gitignored). Free-form verb score, median/worst
     request latency, tokens:
     - `openai/gpt-oss-120b` @ `deepinfra/turbo` — **10/10, 3.5s median, 6.3s worst.** The
       leading role-A candidate.
     - `deepseek/deepseek-v4-flash` @ `fp8` — **10/10 on both arms, 8.9s median, ZERO
       reasoning tokens**, and the cheapest on the slate ($0.09→$0.18). The close alternative.
     - `qwen/qwen3.5-397b` 9/10 @ 8.0s · `z-ai/glm-5.3` 9/10 @ 10.4s · `qwen3.6-35b-a3b`
       10/10 but **37.3s median, 110s worst** — disqualified for interactive use.
     - `deepseek-v4-pro` 9/10 with errors; `llama-4-scout` 7/10 constrained-only, 4.1s.
       `llama-4-maverick` unusable — 8× `HTTP 405` from `deepinfra/base`, an endpoint fault
       (Scout runs the identical code path on `fp8` with zero errors).
     - ⚠️ **Role A is a leading candidate, not a decision** — confirm direct before pinning.
       **Role B is NOT decidable from this run: the bench saturated**, four models at 10/10.
       Its real jobs (derived beliefs, overnight review) do not exist yet.
   - **Same weights + same gateway + different serving tier = 3.7× latency** (12.8s bf16 vs
     3.5s turbo, `gpt-oss-120b`). The third time this project has been misled by treating a
     serving-stack property as a model property. Always compare endpoints, never models.
   - **Constraint tax, where measurable, is ZERO** (+0/+0/+0, one −10). But this does **not**
     retire `Session::constrained`: zero means constraining is *harmless*, and the constrained
     path is the only way to reach an endpoint with no `tools` parameter — which is how Scout
     was benched at all. That is a new argument to keep it.
   - **Plumbing facts for Phase B/C/D, true regardless of which model wins:**
     - Images pass as `data:` URLs, so **no signed-URL subsystem is needed** for blobs behind
       Tailscale. But requests **413 above ~5 MB**, so downscaling (2048px long edge) is
       mandatory, not an optimisation.
     - **Reasoning tokens must be instrumented separately** from completion tokens. They are
       billed and rate-limited, and were ~95% of output on the baseline model.
     - **Verb selection must be measured with a multi-turn agent loop.** Single-turn scoring
       punishes a model for correctly calling `list_types` to explore first, and reports a
       planning model as a failing one.
     - **`ExtractionResult.total` is confirmed a schema+prompt fix** — models populate it as soon
       as `response_schema()` asks for it. Demand digits only; symbol forms break `rust_decimal`.
     - **A vendor namespace is a gateway's spelling, not a fact about the vendor** (2026-09-09).
       Z.ai is `z-ai/` on OpenRouter and `zai-org/` on DeepInfra; DeepSeek, ByteDance, Xiaomi
       and MiniMax all differ the same way. `OPEN_WEIGHT_VENDORS` was written in OpenRouter's
       dialect while production runs direct, so every DeepInfra spelling was refused —
       *including the model the bench script itself defaults to*. Now an alias table keyed by
       vendor, so adding one means adding every spelling.
     - **Native tool calling is a property of the endpoint, not the model** (2026-09-09).
       DeepInfra's Llama-4 endpoints (Scout and Maverick) advertise `response_format` and
       `structured_outputs` but **not `tools`**, so those models can only drive the verb loop
       through the constrained channel. Maverick's 0.7s tool-calling pass in the Phase 0
       bake-off came from an **unpinned** route and describes an upstream we do not use.
     - **Closing our side of the two-channel problem does not close the model's** (2026-09-09,
       found by a live rehearsal, not by a test). A constrained request carries **no** tool
       definitions, yet `openai/gpt-oss-120b` on DeepInfra still returns
       `finish_reason: "tool_calls"` — the stack parses a natively-trained tool syntax out of
       the completion regardless. The verb runs, the answer is right, and the constrained arm
       has silently become a second free-form arm. The `off_schema` canary only inspected
       message *content* and so never fired; it now counts a tool call under a schema too.
       ⚠️ This would have voided both `gpt-oss-120b` rows of the very first slate sweep.
     - **`structured_outputs` varies by quantization tag on one model.**
       `openai/gpt-oss-120b` has it on `deepinfra/bf16` and `deepinfra/turbo` and **not** on
       `deepinfra/fp8`. Confirmed live on the incumbent baseline; the bench pre-flight aborts
       on it rather than producing a constrained half that was never constrained.
   - **Model selection is open and must be decided on merit.** The plan's "27–35B ceiling" came
     from the rejected owned-hardware branch and is void; the genuine frontier of open weights
     runs ~$92–106/month, still under half the €214/mo GPU box that could only hold 35B. Baseline
     numbers from the free Hetzner endpoint are in the archived spike README and are **not** to be
     inherited as constraints.
   - **"One resident general model" is VOID as well.** It was justified by swap latency on a
     single GPU; hosted endpoints are separate and always warm. **Assign a model per job,
     statically — benchmarked by us and written into the code, never chosen by a runtime
     orchestrator** (which is what the latency objection was actually about). Chat wants speed;
     scheduled overnight reviews do not. Benchmarks therefore score **per task**, with latency
     reported separately for interactive vs batch work.
   - **Re-evaluate model selection every 3–6 months** once the assistant is live (user,
     2026-09-07). The landscape moves fast enough that a one-time choice goes stale.
     - **The mechanism already exists in the design:** shadow-mode vetting (plan decision 12) —
       a candidate replays recorded proposals read-only and is scored against what the user
       actually approved. The proposal lifecycle is the eval set (decision 6).
     - ⚠️ **The synthetic bench is a BOOTSTRAP only**, needed because no proposal corpus exists
       yet. Once one does, shadow mode supersedes it. Do not maintain a synthetic benchmark that
       real approval data has obsoleted.
     - Small model + LoRA on the approved-proposal corpus may beat a large model raw, so the
       review compares *after* available optimizations, not raw capability. Requires a provider
       that offers fine-tuning with adapter ownership retained.

4. **Tasks + project management — NOT STARTED, and it gates an assistant capability.**
   Decided 2026-09-07 while reading Phase 0 spike results. omni-me has **no task or project
   records at all**: an inventory of the whole 14,412-event log found journal, notes,
   transactions, auto-import and config, and nothing task-shaped.
   - **Why it surfaced here.** The spike asked the model to work out how badly the user
     estimates work. It answered correctly that **no estimate/actual pairs exist** in the
     corpus — the prose has targets ("out before 7:25"), bare actuals ("took me till 9:20") and
     fixed durations, but never a prediction paired with an outcome. The capability is not
     blocked on the model; it is blocked on the data not existing.
   - **Sequencing, per the user:** build tasks and projects **first**; estimation calibration
     becomes answerable only afterwards. Do not attempt it before then.
   - **This is the verb design paying off, and worth keeping as the worked example.** When the
     record type is declared, the assistant gains the capability with **no assistant code at
     all** — `list_types` starts returning it, `describe_type` explains its fields, `search` and
     `read` reach it. "New features add data, never tools" stops being an assertion here.
   - Related: routines already defer anything beyond a 31-day cadence to "a future task
     feature", so this item also unblocks that cap.

---

## Generalization — the build phases (item 2)

Each phase is its own session per [[feedback-defer-major-phases-to-fresh-session]]. Decisions
are settled; do not re-open them. The invariants a second user is promised are published in
`docs/src/invariants.md`, so **the implementation is checked against that document**, not the
other way round.

- [x] **Phase 0 — records + contract.** Done 2026-09-05. `docs/src/invariants.md` written,
  mdbook + GitHub Pages workflow stood up (mylearnbase pattern), comment convention added to
  `CLAUDE.md`, these records updated.
- [x] **Phase A — config foundation.** Done 2026-09-06. `core/src/config.rs` (closed `ConfigKey`,
  `ConfigValue` Bool/Text/Int, `ResolvedConfig::get` → value + winning `Layer`),
  `ConfigProjection` → `app_config`, `config_overrides.json` for the device layer (never synced).
  Startup reads the materialized table before registering projections. Settings renders a
  collapsed row per key that expands to both layers.
  - **Scope beyond the plan, at the user's request:** the six feature keys ship **inert** (they
    record and sync; nothing reads them — Phase B wires the consumers), and appearance became
    real: a light theme, plus `appearance.accent` with six hues. Blue was never a decision, only
    an inherited Obsidian reference, so it is now data like everything else.
  - **Override widened** from the planned `Option<bool>` to `Option<ConfigValue>` per key, so a
    string-valued key (theme, accent) can be overridden per device too.
  - **Value ordering:** the projection compares the event's authoring `timestamp` against the
    stored row and skips older ones. Config is the one place last-write-wins diverges
    permanently — a setting nobody touches again never self-corrects the way a note does.
  - **Cleared keys keep a tombstone row** (`value = NONE`) rather than being deleted; deleting
    loses the timestamp the guard needs, and a stale `set` would resurrect the old value.
  - On-device (Android) and the two-device override check are **deferred to the next release**,
    by the user's decision. Resolver logic is covered by unit tests meanwhile.
- [x] **Phase B — inert feature toggles.** Done 2026-09-06. The map is enforced in three
  places rather than documented in one: `core/src/config.rs`'s `Feature` enum (exhaustive
  matches, so a seventh feature breaks the build at every site that must decide about it),
  `core/src/events/registry.rs` (feature → projection, with `every_projection_has_an_owner`
  catching a projection nobody claims), and `EventType::authoring_features`
  (`core/src/events/types.rs`) for the write side. Published as `docs/src/features.md`;
  `invariants.md`'s "planned" claims about feature-hiding and theming corrected to shipped.
  - **The command guard is one chokepoint, not ~70 hand-placed checks.** `guard_event_type` in
    `commands/shared.rs` sits in the append tails that `no_command_appends_events_directly`
    already forces every write through, so it covers write paths that do not exist yet (a
    future LLM tool-call layer inherits it). Only outbound HTTP needed explicit guards —
    `box_request` is generic, so it has no chokepoint — and a fourth source-grepping test,
    `every_box_reaching_feature_module_guards_itself`, holds that line. `sync.rs` and
    `update.rs` are exempt by design: sync must work with everything off or a disabled
    feature's events never reach another device, and the updater is how a broken build gets
    replaced.
  - **Trap 1 fixed and regression-tested.** `catch_up` now filters `projection_versions` by
    registered name, in Rust rather than in the query (a wrong-but-parseable `WHERE name IN`
    fails silently as an empty result — the exact failure the fix exists to prevent).
    `a_deregistered_projection_neither_forces_nor_loses_a_replay` was confirmed to fail
    without the fix (6 applies vs 3) and covers the re-enable replay too. The watermark
    *freeze* is deliberately preserved — it is what makes re-enabling replay from the right
    point.
  - **Trap 2 did not apply:** Phase B adds no event types, so `sync.rs:38`'s batch-killing `?`
    is not in play and **no server deploy was needed**.
  - **Decisions taken:** finances gated like any other feature (the user wants it off while
    incomplete); a projection registers when **any** owning feature is on, so `budget` survives
    finances-off + auto-import-on. `boot_features` on `AppState` is a startup snapshot, not a
    live read — a live read would produce a half-off feature between the toggle and the relaunch.
  - **Found while building:** `NotesProjection` serves **three** features, not two
    (`note_llm_processed` routes into either table). `JournalFile` reads `BudgetProjection`'s
    table and is the sole handler of `exchange_rate_recorded`, so dropping it removes FX
    conversion. The auto-import **ticker is server-side**, so the client's auto-import surface
    is one projection plus its commands.
  - ⚠️ **Recorded gap, not an oversight:** the server does not read config, so
    `feature.auto_import = false` on a client does not stop the box's ticker — that is
    `sources.toml` plus the per-source pause flags. Fixing it means registering
    `ConfigProjection` server-side, against its current zero-projection design. Documented in
    `features.md`.
  - **Found by the close-out audit, fixed same session.** The registration seam was only ever
    unit-tested on pure functions; the browser check runs against the mock bridge and never
    touches `lib.rs`, so nothing exercised *stored event → `ConfigProjection` → `app_config` →
    `load_persisted` → registration*. `a_stored_config_event_narrows_the_next_launch_s_registration`
    (`core/src/events/registry.rs`) now covers it headlessly, using the canonical
    `NewEvent::config_set` factory so a payload-shape change cannot pass it; confirmed failing
    against a sabotaged `load_persisted`. The projection list also moved into
    `registry::build_projections` — `lib.rs` had a **second copy** that could have silently
    disagreed with `ALL_PROJECTIONS`. `docs/src/features.md`'s summary line was corrected: it
    claimed all commands refuse, when reads deliberately do not.
  - **Left for the next release** (Phase A's deferral, unchanged): on-device Android and the
    two-device override check.
- [x] **Phase C — record types.** Done 2026-09-06. `core/src/record_type.rs` holds the
  declaration schema (`RecordType`, `PropertyDecl`, `PropertyKind::Text`, `Identity::Date`)
  with a `validate` that rejects any key the `is_complete` scanner could choke on.
  `RecordTypeDeclared` authors under **no** feature and `RecordTypeProjection` is `NEVER_GATED`,
  registered **ahead of** `NotesProjection` so a declaration applies to every journal event
  after it. All four hardcoded sites now read the declaration: `is_complete`, `map_frontmatter`,
  `journal_template::render`, and the properties panel (a loop over `props.entries`, not three
  fixed `ReflectionField`s). Seeded at startup — minimal preset on a fresh log, reflective when
  journal events already exist. Verified: core 632, frontend 115, clippy clean in both wasm
  configs, Playwright 390/1280 with zero console errors. **Absorbed the properties-panel-rework
  backlog item.** [L]
  - ✅ **Gate held, and was proven non-vacuous.** `declaring_the_reflective_preset_changes_no_existing_entry`
    replays a 9-entry corpus of awkward real frontmatter twice — undeclared, then declared — and
    asserts `complete` is identical. Sabotaging `journal_record_type`'s fallback to the minimal
    preset made it fail, confirming the test bites; then restored.
  - Two deliberate deviations from the plan: the declaration is read from the table per event
    rather than cached in an `RwLock` (kills the `clear_tables`-must-reset trap), and
    `journal_template::render` builds through `serialize_journal` rather than a format string
    (the template was an untested second copy of the serializer's safety invariants).
  - ⚠️ **Not shipped:** no UI for editing a declaration — replacing the preset means emitting
    the event by hand. `docs/src/invariants.md` says **(partly today)** for exactly this reason.
  - ⚠️ **Server must deploy before any client that emits `record_type_declared`** — `push_handler`
    400s a whole batch on an unknown event type. This is Trap 2, which Phase B dodged and C does not.
- [ ] **Comment sweep + docs extraction.** Own session. Comment lines run 15–22% of source;
  the biggest blocks are module-header essays (`statement/rendered.rs` 44 lines,
  `auto_import/config.rs` 38, `diagnostics.rs` 35). Route per the four buckets in `CLAUDE.md`.
  Aggressive on chronicle and in-function narration, **conservative on anything phrased as a
  warning** — those are load-bearing. Extracted essays become the mdbook's architecture
  section. Also folds in the root design docs and fixes the README/`architecture.md` claim that
  the server runs projections (it runs none — `server/src/lib.rs:207`). [L]
  - **Publishing the site is deliberately off.** The `Documentation` workflow builds the book on
    every `docs/**` change (catching broken `SUMMARY.md` links) but its deploy job is gated on
    `vars.PUBLISH_DOCS == 'true'`. To go live: enable Pages on the repo (Settings → Pages →
    Source: GitHub Actions — it has **never** been enabled, and `deploy-pages` 404s without it),
    then set the repository variable. Held back because the site is two pages and its intended
    reader — someone deciding whether to adopt omni-me — has no setup guide to follow yet.
- [ ] **Trailing, no deadline.** Preset picker UI, extra presets, record-type export/import;
  the small constants now that there is somewhere to put them (`FORCE_GENERIC_DIRS`, vault
  naming, routine frequency bounds); user-facing setup + customization guides in the mdbook.

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

**Known gap at the stopping point:** the highest-volume CSV institution (four accounts) has
**no oracle at all** — its export carries no balance column. Each of its four dirs in the
cloud backup holds 2 unexamined PDFs; if those are statements, the rendered parser built
2026-09-05 would close the gap. Never checked. (Institution named in the overlay's tracker.)

**Both bank sources are now OFF at the source (2026-09-07), not merely paused.** Done by
renaming their credential section headers on the box so the overlay stops registering them —
neither credentials view uses `deny_unknown_fields`, so a renamed section parses and is
silently ignored, and a source is registered only when its section is `Some`. The scheduler
now boots `sources=0` instead of `sources=2`. Values are untouched and it reverses by renaming
two lines back. Institution names, exact paths and the backup file are in the **overlay's**
`tasks.md` — they do not belong in this repo.

This was needed because **pausing does not survive a restart**, which had gone unnoticed:

- [ ] **The pause off-switch cannot persist if the XDG config dir is not writable.** Found
  2026-09-07; ⚠️ **the defect is in the deploy image, not in finance code**, so it will bite any
  future auto-import source. `paused::set_paused` writes `paused_sources.toml` into
  `$XDG_CONFIG_HOME/omni-me/` via temp+rename. When a container mounts only
  `credentials.toml` into that directory, the directory itself is created by Docker as a mount
  parent and is **root-owned**, while the app runs unprivileged — so the write fails
  `Permission denied (os error 13)`. `pause_source_handler` correctly surfaces that as a 500
  rather than faking success (#367's whole point), but the net effect is that pause works
  *live* and never survives a restart, and every boot logs `paused=0` with the sources running
  again. Worth a startup preflight that warns when the config dir is not writable, since the
  failure is otherwise only visible at the moment someone tries to pause. Image-side fix and
  the operational record are in the **overlay's** `tasks.md`. [XS]
- [ ] **Reach import parity with paisa's seven importers.** Parity map is written (overlay
  `IMPORT_PARITY.md`; institution names are private). **Four of the seven are now covered**
  as of 2026-09-05: the old comma-splitting `statement_csv.rs` is deleted, imports run
  through `core/src/statement/`, and both rendered-PDF layouts parse — verified over 136 real
  files with zero self-check failures. Remaining: the two other CSV institutions and the
  investment/holdings shape, which is a genuinely different problem (positions, not cash
  rows). paisa's importers are in the cloud backup with the corpus in sibling dirs. [M]
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
- [x] **Cold-start race: startup invokes fire before `AppState` is managed, and the feature set
  never re-derives.** **FIXED + VERIFIED ON DEVICE 2026-09-07 (v1.1.1 + v1.1.2).** The phone's own
  problem report on 1.1.2 shows all seven commands *recovering* rather than failing —
  `get_workspace` 567ms, `get_config` 573ms, `get_timezone` 595ms, `get_runtime_profile` 664ms,
  `take_pending_share_intent` 649ms, `get_sync_status` 649ms, `list_known_accounts` 853ms — each
  **after 1 retry**, every entry a `warn` instead of a failure, against seven hard
  `state not managed` failures on the same device that morning. The Finances tab is correctly
  gone, and `surface` came back clean through the updater's auto-restart (the only path that
  reproduced the splash hang). ⚠️ **The race still happens — it is handled, not eliminated**, and
  the recorded timings are the early-warning signal: one retry at <1s means the 10s deadline has
  wide headroom, and these numbers climbing (bigger log, slower device) is the cue that the
  margin is eroding before it fails open again. Original entry follows.
  Found 2026-09-07 on the phone during the v1.1.0 verification pass, and
  **confirmed from the app's own diagnostic ring buffer** rather than inferred — the first real
  use of the feedback feature, which is what surfaced it. [S–M]

  **Symptom.** `feature.finances` was set off (shared) on `surface` and took effect there after a
  restart. On the phone the Finances tab stayed, across an app restart *and* a device reboot,
  while Settings showed `shared: off` / `this device: follow` and the collapsed row read **Off**.

  **Root cause, from the ring buffer:** seven startup commands failed at +0.6–0.7s —
  `get_workspace` and `get_timezone` with `__ipc_timeout__`, then `get_runtime_profile`,
  **`get_config`**, `take_pending_share_intent`, `get_sync_status` and `list_known_accounts` with
  *"state not managed for field `state` … You must call `.manage()` before using this command"*.
  The WebView fires its startup invokes before `setup()` finishes (DB connect, WAL recovery of
  ~36k batches, `init_all`, record-type seeding) and reaches `handle.manage(AppState{…})` at
  `lib.rs:798`. `get_config` failing takes the `let Ok(entries) = … else` branch at
  `main.rs:627`, which sets `Features::default()` — **everything on** — and `features.rs:27`
  sets that signal *exactly once*, so a lost race persists for the whole session.

  **Why it stayed hidden until now.** Before feature toggles existed, everything was on, so the
  fail-open fallback was always *correct*. The race almost certainly predates v1.1.0 and only
  became visible the first time a feature was actually switched off. The phone loses a race
  `surface` wins because its cold start is slower.

  ⚠️ **Fail-open is deliberate and should stay** — `types.rs:243`: "A missing key must never hide
  a tab: an empty or failed read would blank the nav." The consequence is that a drawn tab is
  the app's *error state as well as its on state*, indistinguishable from outside. The fix is
  recovery, not a stricter default. Also note the race was **already known**: the IPC timeout
  helper exists for it and says so (`bridge.rs:422`, "the signature of the boot-race this helper
  exists for"). What is missing is a retry, not detection.

  **Fix directions (not chosen):** emit a backend-ready event after `manage()` and have the
  frontend re-derive features on it; or manage a lightweight ready-flag state early so commands
  can await initialization instead of hard-failing; or retry `get_config` in the background and
  correct the signal after the splash lifts (needs care — `features.rs` requires set-once, and a
  mid-session feature flip is what `boot_features` being a snapshot deliberately avoids).

  **Open question, deliberately unresolved:** whether the phone's `boot_features` also ended up
  on. "The tab still shows data" does **not** settle it — the finances pages have a
  stale-while-revalidate read cache (Stage C3), so they can render cached rows whatever the
  backend would now answer. Needs a probe that bypasses the cache.

  **⚠️ SECOND, WORSE VARIANT — the desktop post-update splash hang (v1.1.2 fix).** Same root
  cause, one stage earlier, and *unrecoverable* rather than cosmetic. When the **IPC handler
  itself** is not ready (as opposed to `AppState` not being managed), Tauri does not reject —
  the invoke is silently **dropped**, its promise never resolving and never rejecting. A plain
  `invoke` awaits it forever, so `features_ready` never flips and the splash never lifts. Seen
  on `surface` after the updater's auto-restart on both the 1.1.0 and 1.1.1 updates; closing and
  reopening (a launch where IPC is ready in time) clears it every time. **Verified live while
  hung:** backend fully initialized, `finances off` honoured, auto-pull applying with `failed=0`
  two minutes in, WebView alive and burning CPU, no DB contention — a healthy backend under a
  dead UI. The retry added in 1.1.1 cannot help here: it retries on errors, and no error is ever
  produced. Only `invoke_timed` converts "never settles" into an `Err`, and despite its doc
  saying *"use this for anything fired during app boot"*, **only `get_workspace` and
  `get_timezone` had adopted it** — `continuity.rs` was immune for exactly that reason.
  Fix (1.1.2): `invoke_get_config_timed` plus the same bounded retry/deadline loop
  `continuity.rs` already uses (500ms attempt, 100ms gap, 15s fail-open cap). The two halves are
  both required — timed-alone fails open with the wrong feature set, retry-alone never fires.
  Also added a `tracing::debug!` on the backend `get_config` entry, because nothing on the boot
  path logged and the dropped-IPC diagnosis had to be *inferred*; that line makes it observable.
  Housekeeping: 1.1.1 introduced a duplicate `sleep_ms` in `bridge.rs` — removed, now uses the
  pre-existing `crate::timer::sleep_ms`.
- [ ] **`GET /feedback` is broken — the read side of feedback capture has never worked against
  real data.** Found 2026-09-07. The endpoint returns a SurrealDB parse error:
  *"Missing order idiom `timestamp` in statement selection"* — the query is
  `SELECT meta::id(id) AS eid, device_id, … WHERE event_type = 'feedback_captured' ORDER BY
  timestamp DESC LIMIT $limit`, and SurrealDB requires an `ORDER BY` field to appear in the
  selection. ⚠️ **It is served as HTTP 200 with `content-type: text/plain`**, so a caller cannot
  distinguish the failure from an empty result — fix that too, not just the query.
  The **write** path is fine: the report synced and was read back off the box via `/sync/pull`
  (screen `journal:day`, platform android, 1.1.0, 10 recent errors, 20 recent events, 3.3 KB).
  [XS]
- [ ] **`chunk_for_push` cannot get a single oversized event under the byte cap** — noticed while
  reading, **NOT observed**; today's stall was a tailnet drop, not this. Recorded so it is not
  re-derived. `chunk_for_push` splits by event count and cumulative bytes, but its own comment
  says "An event larger than the byte budget is still emitted alone (best effort)". The server's
  `DefaultBodyLimit` answers **413**, and `SyncError::Rejected` — the quarantine-don't-retry path
  whose doc says "the same bytes will be rejected forever" — is keyed on **400**, so an oversized
  event lands in retry-with-backoff instead. `feedback_captured` is the one event type with no
  natural size bound (`screen_data` is "whatever the page rendered of itself", `recent_errors` up
  to 50×500). Real payloads seen so far are ~3 KB, so this is latent, not urgent. [S, unverified]
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
  `~/.claude/plans/lets-continue-eventual-aurora.md`. **Stage 2 built 2026-09-05** (see below).
  Remaining: the live-box end-to-end, and the panic-path follow-up.

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

- [x] **Stage 2 — the diagnostic ring buffer. BUILT 2026-09-05.** `frontend/src/diagnostics.rs`:
  a 50-entry `thread_local!` ring (each entry capped at 500 chars) fed by four producers — a
  chained panic hook, `window` `error` and `unhandledrejection` listeners, and a
  `console.error`/`warn` patch. Backend half is `EventStore::get_recent_by_device(device, limit)`
  plus `recent_event_lines` in `commands/feedback.rs`, which formats the last 20 events
  **server-side** so the list never crosses IPC and cannot be forged by a client. Payloads are
  never included. Both payload fields were already plumbed, so no wire-format change.

  **A panic flushes the ring to `localStorage`; nothing else does.** wasm has no unwinding, so a
  panic traps the module and the most valuable entry is the one an in-memory ring cannot deliver.
  Boot restores that key as `[prev]` entries and clears it — a crash trail survives exactly one
  relaunch. Per-error persistence was rejected as the churn `screen_context.rs` avoids.

  **The console patch is built in JS, not bound as a Rust closure**, because `console.error` is
  variadic and a Rust closure cannot reach JS `arguments` — binding one would silently drop every
  argument past the first. Confirmed live: dx's own dev-server console patch wraps *ours* rather
  than orphaning it, because `install()` runs before `dioxus::launch` and the wrapper chains via
  `orig.apply`. Ordering matters; do not move the install site.

  **Verified:** 7 new ring tests (101 frontend total), the new store test (690 core), 82 src-tauri
  incl. architectural guards, 8 feedback route tests incl. 2 new ones covering the Errors and
  Recent-events markdown sections, both wasm clippy configs. Playwright at **390 and 1280**:
  clean boot shows no error line at all, one `console.error` → "1 recent error", `warn` also
  taps, drop toggle strikes through and restores, and **the send flow ran at 390** and returned an
  id. Console clean apart from the known `showDXToast` dx artifact. Device-only legs are folded
  into the next release test pass, below. [M]


- [ ] **Feedback: the two device-only legs, folded into the next release test pass**
  (user, 2026-09-05 — test all of this at once rather than piecemeal). Neither can be proven in
  the browser, and both are cheap once a build is on a phone.
  **(1) Cross-device end-to-end** — capture on the phone, then `GET /feedback` from the desktop
  and see that report. This is the leg that proves the loop; everything so far ran against the
  mock bridge. **(2) A real panic** — the `localStorage` crash flush and the `[prev]` restore in
  `diagnostics.rs` are verified by reading the code, not by a wasm panic. It is the branch that
  matters most when it fires and the only one no test touches. [S]

- [ ] **Notes search publishes only its coordinate, not its query** (2026-09-05). A
  `notes:search` report says which surface was open but not the query text or the result count —
  which is the whole content of a "search found nothing" report. The query lives in a child
  component, so this needs the describer to reach it or the child to publish. [S]

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
- [ ] **A new projection never sees history, and "ignored" is recorded as "applied".**
  Observed end-to-end during the v1.1.0 two-device upgrade, 2026-09-07 — **benign this time**,
  but the mechanism is general and will recur on every future projection.

  **What happened.** `surface` upgraded first and seeded `record_type_declared` (10:28:04Z).
  The phone, still on 1.0.5, pulled it: no projection matched, every one returned `_ => Ok(())`
  (`notes_projection.rs:78`), so `apply_events_resilient` saw no error, counted the event as
  **successfully applied**, and advanced the bookmark past it. The phone then upgraded and
  registered `record_types` for the first time — a brand-new projection, whose watermark
  `init` seeds to `time::now()` (`projection.rs:103`, `last_received_at ?? time::now()`).
  `catch_up`'s `min` was already past 10:28:04 because the no-op had advanced it, so nothing
  replayed. `load_record_type` queries the **projection table**, found it empty, and the phone
  declared a second time (10:54:45Z).

  **Why it was harmless.** `seed_journal_record_type` derives the preset from the *event log*
  (`SELECT id FROM events WHERE event_type = 'journal_entry_created'`), not from local context,
  so both devices independently answered the same durable question, both chose
  `journal_reflective`, and last-write-wins converged on identical content. The dangerous
  variant is a **fresh install** with no entries seeding `journal_minimal` and winning LWW
  against upgrade devices — which is what `complete` failing closed on an empty required list,
  and `auto_close: false` on the minimal preset, exist to contain.

  **The conflation to fix.** `_ => Ok(())` cannot distinguish "not my type, a sibling handles
  it" (constant and correct) from "no projection in this binary handles it" (a future event).
  The oracle already exists: `EventType::from_str` ends `other => Err("unknown event type")`
  (`types.rs:160`) — that is the binary's complete vocabulary, independent of the projections.
  Note the asymmetry with feature toggles, which *are* handled correctly: a de-registered
  projection's watermark **freezes** and re-registering replays what it missed
  (`a_deregistered_projection_neither_forces_nor_loses_a_replay`). Replay safety keys on
  "existed and was switched off", not on "did not exist yet".

  **Two directions — user leaning toward either, NOT yet decided (2026-09-07):**
  - **A. Unclaimed-event side table.** Keep the watermark advancing; when `from_str` fails,
    record the id in `unclaimed_events`. On startup replay rows whose type now parses, then
    delete. Dead-letter-queue pattern. Needs an age-out rule or a device pinned on an old
    version accumulates rows forever. Handles the general case: *the event arrived before any
    handler existed.*
  - **B. Stop conflating "new projection" with "new schema field".** The `?? time::now()`
    comment describes an *existing* projection upgrading into a newly-added field, where
    `now()` is right — but the same line also catches *genuinely new* projections, where it is
    wrong. They are distinguishable: a pre-existing row has a non-empty `last_event_id`, a
    fresh one gets `''`. Seed only genuinely-new projections from epoch. Smaller, no new
    machinery, and it covers the case actually observed.

  ⚠️ **Measure before designing.** B's cost is the "surprise full replay" the comment avoids —
  but the log is ~14,360 events and a boot recovered 36,670 WAL batches in ~0.6s, so a
  one-time replay may be seconds. If it is, B is a few lines. Nothing here risks data: events
  are appended durably *before* apply, so every failure mode is "not yet derived", never "not
  stored". [S–M, design call]
- [ ] The objection is **open-ended gate config**, not automation and not email as a source. Any replacement has to make "which mail is relevant" self-maintaining. Directions worth *only* a note until v1 ships — do not design these now: forward-to-a-dedicated-address push instead of polling (the user's filter action becomes the signal, no label to maintain); Gmail API query search instead of a maintained label; or drop email entirely and add API sources. Whatever replaces it inherits the constraint at `receipts.rs:216-231` — untrusted sender input reaching an LLM, with the pending-review queue as the only control. [→ own planning session, [[feedback-defer-major-phases-to-fresh-session]]]

- [ ] **Journal line timestamps — redesign, design-first** (user, 2026-07-05). Its own
  session. ⚠️ **The body of this item was lost** — in the pre-reconciliation `tasks.md` this
  was a bare heading with unrelated items filed beneath it, the same corruption that left an
  orphaned paragraph in the friction log. Only the title and the design-first framing
  survive; the requirements need re-eliciting from the user before anything is built. [M]
- [x] **Journal template hardcodes the user's personal journaling framework** — **RESOLVED 2026-09-06 by Phase C.** All four sites now read a declared record type; the user's three prompts ship as the *reflective preset*, seeded only where journal entries already exist. Original entry follows. `frontend/src/journal_template.rs::render` bakes in the user's own choices: the three reflection property keys (`homework_for_life`, `grateful_for`, `learnt_today`), the `## What happened today?` section heading, and the `daily_note` tag. Those same three keys are also hardcoded in the day-complete `is_complete` check (`core/src/events/notes_projection.rs`), and the `tags: [daily_note]` inline-list form is itself a workaround for that parser — so the template, the auto-close logic, and the typed properties panel (`journal.rs::JournalPropertiesPanel`, 3 fixed reflection fields) are all coupled to this one personal schema. Generalizing (user-configurable reflection prompts + template) means reworking `is_complete` to not key off fixed names + adding a config surface for the prompt set. Also a mild personalization-in-open-core smell (personal journaling prompts sit in the public repo, though not identity/financial data). [M] — flagged by user 2026-07-06. **SUPERSEDED 2026-09-05: this is now Phase C above.** The estimate was wrong ([L], not [M] — `JournalProps`' three named fields become an ordered map, which ripples through serialization and every test naming a field), and the resolution is not "make the prompts configurable" but declared record types, with the user's three prompts shipping as the *reflective preset* rather than the app's default.
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
- [ ] Configurable `FORCE_GENERIC_DIRS` (hardcoded to `Work/`). [S] — **folded into
  generalization's trailing work**; it stalled this long because there was nowhere to put the
  config, and Phase A creates that.
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
