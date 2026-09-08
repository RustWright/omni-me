# NEXT

**Next action: verify Phase A end-to-end against a throwaway hub.** Code is built, tested and
clippy-clean; the sync round trip is NOT yet proven. **No sudo, no docker** — `server`'s
`DB_PATH` is *relative*, so `cd <scratch dir> && cargo run -p omni-me-server` is an isolated
hub (teardown = `rm -rf` it); credentials are optional by the zero-config guarantee. Seed via
`/sync/push`; cold-start the agent and check it backfills; have it author one event and
confirm a second agent instance receives it carrying the first's device id; with a feature
off, confirm authoring is refused while that same event arriving *inbound* still applies.

## Decisions in force — inherit, do not re-derive
- **`EventWriter` (`core/src/events/writer.rs`) is the only way to author an event** — guard +
  append + project + push-nudge, welded. Four sites used to do those separately.
- **Cold start pulls config BEFORE choosing projections** (user, 2026-09-08). An empty DB
  resolves every feature to its default of `on`, and for an agent a cold start is normal, not
  rare. Costs one extra read of the log; correct from the first boot.
- **`JournalFile` is excluded structurally**, via `registry::FILESYSTEM_PROJECTIONS` — not
  left to the finances switch, which does not exist yet at cold start.
- **Test isolation is mandatory.** The log is real and append-only: anything that writes runs
  against a throwaway hub. The real box is touched **only** under `--read-only`, which
  refuses to build a writer at all. Agent runs on `surface`; deploying it to the box is out
  of scope, gated behind the box's missing swap + container memory limits.
- Model choice **DEFERRED to Phase D** via shadow mode; dev endpoint is provisional, never a
  selection. Assign a model **per job, statically**. "27–35B ceiling" and "one resident
  model" are **VOID**. `docs/src/assistant.md` is the contract.
- **Agent-to-agent is the verification shape** (user, 2026-09-08). A real Tauri client as the
  second device is NOT needed for the initial tests. The test hub's `0.0.0.0:3000` bind is
  accepted as-is — do not spend time narrowing it.
- ⛔ **FINANCES DEFERRED INDEFINITELY.**
- **Do not re-survey:** generalization · v1.1.x pass · hosting market survey · RAM sizing ·
  the model bake-off (n=1 per task — ranking is NOT trustworthy).

## Open threads
**Move the append scan into `core`, workspace-wide.** It sits in the tauri crate and reaches
`agent/src` by path traversal — works, wrong home. Blocked on triaging **5 real bypasses**:
`auto_import/{subprocess,csv,imap_source,rest}.rs` + `llm/pipeline.rs` (all finances/LLM, so
⛔ gates them). `sync/client.rs`, `server/routes/sync.rs`, `events/writer.rs` stay exempt.
Read-only rehearsal vs the real box · untouched by Phase A: `ExtractionResult.total`, `GET
/feedback` (200 text/plain), server-side `feature.llm` · server 1.1.0 vs overlay lock 1.1.2 ·
`chunk_for_push` 413 · history gap A/B · tasks gate estimation · curiosities+memory prune.
