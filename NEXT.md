# NEXT

🔴 **RULING 2026-09-25 — the sender list must stop being a GATE.** The user will not edit a list,
rebuild the app, or forward mail to himself for a new vendor to be processed. ⛔ **Never add a
vendor to fix a miss** (an `uber` patch was offered and was the wrong shape). ⛔ **Manual
forwarding is a testing workaround, not a design.** Verbatim + evidence: memory
`project-receipt-sender-list-should-learn`, overlay `DEV_TESTING.md` § "THE SENDER GATE IS THE BUG".

## ▶ NEXT ACTION — design the gate inversion (design-first, its own session)
⚠️ This changes the ingestion path; plan before coding. The shape, not yet agreed:
1. **The classifier he assumed was deciding already exists and is OFF** — the archive's document
   `kind` comes from the enrichment pass / C2 reader seat, and dev runs `OMNI_ENRICH_ENABLED: "0"`,
   so every archived email reads `kind: null`. ⛔ Move the decision there; do not build a third one.
2. `accepts()` degrades to one signal among several + a skip-list for known noise. A cheap
   structural pass (currency amount? order/total keyword?) can gate the expensive extractor, so
   cost stays bounded without a name list. ⚠️ Cost is the only real argument for a pre-filter.
3. ⛔ **Also deterministic:** stop keying on a model field. Parse the order id from the URL the
   vendor already prints (`instacart.ca/store/orders/0906<id>`, `walmart.ca/en/orders/<id>`);
   `preprocess` already extracts URLs. Model becomes fallback. See memory
   `project-model-derived-keys-are-nondeterministic`.

## 🔴 Pending right now
- **Fresh Walmart/Instacart confirmation+receipt forwards** were sent ~14:05Z; the poll cycle runs
  ~:19/:20/:22 past each half hour. ⚠️ Read the tick, then the phone over CDP.
- **Remove `[server].auth_token` from `/etc/omni-me/credentials-dev.toml` at the next box reset**
  (user, 2026-09-25). ⛔ Never touch the live credentials file. ✅ Safe: both instances bind only the
  tailnet address, never `0.0.0.0`, so the tailnet is the auth boundary. Doing it sooner is what
  unblocks `pm clear` on the phone (a clear wipes the app's token).

## ⛔ Inherit — settled
- ⛔ **Grouping runs PER DEVICE.** ✅ Verified on the phone: order-scoped keys where a reference
  parsed, `receipts-uid-N` otherwise. ⛔ "One Walmart order → six batches" is NOT duplication.
- 🔴 **Grouping merges only SOMETIMES** — same email gave `order_ref: "."` at conf 0.93 on a
  re-run. ✅ It failed safe (two review items, not a wrong merge) and `group_key()`'s 4-char floor
  caught it. ⛔ Do not loosen those bounds. ⚠️ One green run is not evidence a key is stable.
- ⛔ **Never bump a projection version** until the rebuild defects are fixed (`tasks.md`).
- ⚠️ He forwards `gmail_personal` → `gmail_work`; vendor mail *arrives* at `gmail_personal`.

## 🔴 Open
- 🔴 **Phone wedge** — 8h on `Restoring 16052 events…`, all runtime threads `state=S`, every Tauri
  command hung; a force-stop recovers it. ⛔ Volume disproven (the S9 replays 17,148 events in
  104s). Stall location unknown. The new chunked apply logs per chunk; repro needs `pm clear`.
- 🔴 Three rebuild defects · projection writes swallow statement errors (four have ZERO `.check()`)
  · suite deadlock (recorded fix disproven) — all in `tasks.md`.

## ⛔ Standing
- ⛔ **NEVER run the full suite on this box** — it OOMs; `CARGO_BUILD_JOBS=1` does not save it.
  Local = fmt/clippy/check. Draft PR #1 is open, so a plain push runs the full gate in ~8min.
- ⚠️ `build-dev-apk.yml`'s `public_ref` must be a BRANCH. ⚠️ `omni-me-private` is NOT hook-committed.
  Branches: `dev/role-split-model-seats` / `dev/untested-overlay`.
