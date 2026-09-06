# NEXT

**Next action: generalization Phase B — inert feature toggles.** ⏰ The only deadline-bearing
item: it lands before the next feature ships, or every feature added this year is a retrofit
owed. **Phase A shipped 2026-09-06** — config foundation, light theme, accent hues. Phases in
`tasks.md` § Generalization; reasoning in `~/.claude/plans/lets-continue-deep-blanket.md`.

## Decisions in force — inherit these
- ⛔ **FINANCES ARE DEFERRED INDEFINITELY.** Don't start, propose, or "just fix" an item. State
  only: tab offline, both bank sources OFF, categorization to `Unmatched`.
- **`docs/src/invariants.md` is the contract** — check against it, never back-fit it.
- **Client customization is data; server customization may be code.** From fork cost: a client
  fork means NDK + signing + your own `/updates`.
- **Declared record types are the target** (Phase C). **No new code may assume one journal type.**
- **"Off" means inert**: no tab, commands refuse, projections and schedulers never start;
  reversible by replay. Features do **not** map 1:1 onto projections, so an explicit feature →
  (tab, projections, schedulers, commands, settings) map is required.
- **UI stops at schema-driven forms** — no view DSL, no UI plugins.
- **Config is built and settled.** `core/src/config.rs`: closed `ConfigKey`, `ConfigValue`
  Bool/Text/Int, `ResolvedConfig::get` → value + winning `Layer`; `ConfigProjection` →
  `app_config`; device overrides in `config_overrides.json`, **never synced**. Startup reads the
  materialized table *before* registering projections ⇒ a value set elsewhere applies at next
  launch. Adding a key is one enum arm plus its match arms — no migration.

## Phase B's two traps, both found while building A
- ⚠️ `ProjectionRunner::catch_up` (`core/src/events/projection.rs:148`) takes the min watermark
  over **every row** in `projection_versions`, unfiltered by what is registered. The moment B
  stops registering a disabled projection, that frozen row replays the whole log every launch.
  **Filter that query by registered name.**
- ⚠️ `server/src/routes/sync.rs:38` `?`s out on the first unknown event type, so one unrecognised
  event **400s the whole push batch**. Deploy the server before running a client that emits a new
  event type; until then test under `OMNI_DATA_DIR`, which forces localhost.

## Do NOT re-survey
Generalization design is CLOSED. The server is already open-core and runs zero projections.
Verification PNGs are throwaway.

## Open threads
146 raw palette utilities remain in `finances.rs` — light works there, its error/edge states look
off; left alone under the finance hold. · Android `themes.xml` pins a charcoal `windowBackground`,
so light likely flashes dark at cold start — unverified; on-device and the two-device override
check are both deferred to the next release. · Never `cargo test -p A -p B`. ·
`npm run copy:editor:dev` after every dx rebuild. · Curiosities→concepts + memory prune owed.
