# NEXT

**Next action: pick up Phase B (verbs + model), or clear an open thread.** Phase A is
**verified end-to-end** — all four checks passed against a throwaway hub on 2026-09-08:
cold-start backfill, agent-to-agent with the author's device id preserved, authoring
refused with the feature off, and that same event still applied when inbound. The agent
now has `--probe` (author one throwaway note through the real writer) as the twin of
`--read-only`. Nothing about the round trip is unproven any more.

## Decisions in force — inherit, do not re-derive
- **`EventWriter` (`core/src/events/writer.rs`) is the only way to author an event** — guard +
  append + project + push-nudge, welded. Four sites used to do those separately.
- **Cold start pulls config BEFORE choosing projections** (user, 2026-09-08). Verified live;
  it only actually worked after the watermark fix below.
- **`--probe` is permanent** (user, 2026-09-08), not scaffolding: an ops diagnostic for
  "can this host author and sync?". It writes a real note, so throwaway hubs only.
- **`JournalFile` is excluded structurally**, via `registry::FILESYSTEM_PROJECTIONS`.
  Confirmed absent from the agent's registered list at runtime.
- **Test isolation is mandatory.** The real box is touched **only** under `--read-only`.
  `OMNI_AGENT_DATA` doubles as the non-production switch, so an isolated agent structurally
  cannot resolve the box's URL.
- Model choice **DEFERRED to Phase D** via shadow mode. Assign a model **per job,
  statically**. "27–35B ceiling" / "one resident model" are **VOID**.
  `docs/src/assistant.md` is the contract.
- ⛔ **FINANCES DEFERRED INDEFINITELY.**
- **Do not re-survey:** generalization · v1.1.x pass · hosting market survey · RAM sizing ·
  the model bake-off · **the four Phase A checks — they passed, don't re-run them.**

## What the verification actually found
**A silent, permanent projection bug — fixed, with a regression test.** `init_all` seeded a
first-sight watermark to `time::now()`, which lands *after* events already in the store. So
the agent's cold-start pull (`pull_only` appends without projecting) was never projected:
config never materialized, and the agent booted on default features no matter what was
configured. The mark stayed ahead forever, so no later launch recovered it. Fix distinguishes
**no row** (never ran → seed epoch, replay the log) from **row present, field NONE** (upgrade
→ seed now). Still latent for any *newly-shipped* projection on an existing install; the
client's fresh-install path was never affected, because it calls `init_all` before its first
pull, on an empty log.

## Open threads
Pusher now logs success *and* failure — `PushEvent` was only consumed by the Tauri UI, so
agent-side push failures had been invisible. · **Move the append scan into `core`**: it sits
in the tauri crate, reaching `agent/src` by path traversal. Blocked on triaging 5 real
bypasses: `auto_import/{subprocess,csv,imap_source,rest}.rs` + `llm/pipeline.rs` (⛔-gated).
`sync/client.rs`, `server/routes/sync.rs`, `events/writer.rs` stay exempt. · Read-only
rehearsal vs the real box · untouched by Phase A: `ExtractionResult.total`, `GET /feedback`,
server-side `feature.llm` · server 1.1.0 vs overlay lock 1.1.2 · `chunk_for_push` 413 ·
history gap A/B · tasks gate estimation · curiosities+memory prune.
