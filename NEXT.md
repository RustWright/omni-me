# NEXT

**Next action: generalization Phase C — record types.** The last structural phase; scope and
gate in `tasks.md` § Generalization, reasoning in `~/.claude/plans/lets-continue-deep-blanket.md`.
**Phase B shipped 2026-09-06** — feature toggles are live and enforced.

## Decisions in force — inherit these
- ⛔ **FINANCES ARE DEFERRED INDEFINITELY.** Don't start, propose, or "just fix" an item. It
  now has a real off switch, `feature.finances`, rather than a convention to remember.
- **`docs/src/invariants.md` is the contract** — check against it, never back-fit it.
- **Client customization is data; server customization may be code.** **UI stops at
  schema-driven forms** — no view DSL, no UI plugins. **Declared record types are the target**
  (Phase C); **no new code may assume one journal type.**
- **Config (A) and the feature map (B) are settled** — closed enums with exhaustive matches, so
  a new key/feature breaks the build where it must be decided; a new **event type** declares its
  owner in `EventType::authoring_features`. A projection registers if **any** owner is on.
- **Feature changes apply at next launch.** `AppState.boot_features` is a startup snapshot on
  purpose; a live read would half-disable a feature between the toggle and the relaunch.

## Phase C's gate, and what Phase B leaves behind
- ⚠️ **Before any of C merges:** replay the log with the shipped default record type; assert
  `complete` is unchanged for every existing journal entry.
- Phase B's traps are **closed**: `catch_up` filters by registered name (regression test
  confirmed failing without it); no new event types, so **no server deploy is pending**.
- ⚠️ **Known gap, documented not fixed** (`docs/src/features.md`): the server does not read
  config, so `feature.auto_import = false` on a client does not stop the box's ticker.

## Do NOT re-survey
Generalization design is CLOSED; the feature map is built, tested and published — don't
re-derive it. The server runs zero projections.

## Open threads
⚠️ **Disk 91% full / 9.9GB free at 2026-09-06 close** (32GB `target`, 6.2GB
`frontend/target`) — fine for cargo, **reclaim before any Android or release build**. · The
user wants finances switched off on their own device now that the switch exists. · 146 raw
palette utilities in `finances.rs`; light theme looks off there, left under the finance hold. ·
Android `themes.xml` pins a charcoal `windowBackground`, so light likely flashes dark at cold
start — unverified; that and the two-device override check are deferred to the next release. ·
Curiosities→concepts + memory prune owed. · Never `cargo test -p A -p B`; `npm run
copy:editor:dev` after every dx rebuild.
