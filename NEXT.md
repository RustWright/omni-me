# NEXT

**Immediate: launch two overnight runs** (agreed, user asleep). Binary is current — ⛔ no rebuild.
1. **A/B slate re-run** — `scripts/bench-slate.sh`, 9 models, ~2.5h. Hub `omni-hub` is up + seeded.
2. **C1 extraction** — `scripts/bench-c-slate.sh extraction <dir> <C3 finalists>`.
⚠️ C1's abstention arm will print **skipped**: `OMNI_BENCH_ABSENT` has no source, the corpus is all
statements. ⛔ Seat D deferred by agreement — it needs a runner that does not exist (D uses the
*text* client via `OMNI_AGENT_LLM_MODEL`, so `bench-c-slate.sh` cannot drive it).

## ✅ Decided
- **Seat C3 = `deepseek-ai/DeepSeek-V4.1-Flash`** (user, 2026-09-16). Runner-up
  `Llama-4-Maverick` — equal coverage, **2× faster**, kept on file for the next comparison.
  ⚠️ Overrode the strict lexicographic order on the record: `Ling` won key 1 by **one figure in
  780** (noise) but structurally cannot read **11 of 30** documents. ⛔ Primary/fallback routing
  was evaluated and **rejected** — no measurable quality gain, ~$1 total saving, slower.
  Full reasoning: `MODEL_THRESHOLDS.md` § Seat C3 + `MODEL_BENCH.md` Part 9.
- **Seat A's latency gate reads successful requests only**, and is **median ≤ 15s / worst ≤ 30s**
  — renamed from "p95" because at 14 cases nearest-rank p95 *is* the maximum (R16). `bench.rs` now
  splits successful from failed latency and reports both.

## ⏳ C2 ran, NOT analysed — do this first
14 models, `runs/20260915-210805-c2/`. ⛔ **Only 3 populate `fields` at all**: `gemma-4-26B`
(fab 3, 18/21), `gemma-4-31B` (fab 5, **23/23**), `GLM-5.3-Flash` (fab 5, 13/21). Lexicographic
order gives `gemma-4-26B`; ⚠️ denominators differ (21 vs 23) from errors, so it needs the same
intersection treatment `c3-compare.py` does for C3 — **no c2 equivalent exists yet**.
- 🔴 **R21: the zeros are two different causes.** Three models answer in 5–16s with **zero errors**
  and simply omit the array — `fields` is not in `document_schema`'s `required` and the prompt
  frames it as discretionary. Three others are **timing out** (R20). ⛔ Never report these as one.
- ⚠️ **`DeepSeek-V4.1-Flash` wins C3 and scores 0/21 on C2.** The seats are different jobs.

## ⛔ Inherit — do not re-derive
- 🔴 **The 2026-09-15 A/B slate is VOID** (R13) — the connection died mid-run. R15 retracted for
  the same reason. ⛔ Never quote the 9.4%. The re-run above is the replacement.
- 🔴 **Role C has no `max_tokens` (R20), no 429 retry (R22), no request spacing (R12).** All three
  on `extraction/openai_compat.rs`; the chat client has them and they were never shared across.
  ⛔ A 429 count on this path is not an endpoint property.
- ⛔ **Rank C3 with `scripts/c3-compare.py`**, never the per-model cards (R17) — denominators
  differ and the bias has **no consistent sign**. `+dir` merges a retry pass; bare args filter.
- ⚠️ **Repeated failure this session, 4×: quoting a rate before the denominator exists**, and 3×:
  a string transform that was *nearly* right. ⛔ Write the mechanism, withhold the ratio.
- ⛔ **Closed:** catalogue retrieval · blob store · archive page · `imap_real.rs` · Phases D–G ·
  finance propose · role wiring · 413 hazard · enrichment · C2/C3 build. New product work
  (R17–R22) is in `tasks.md`; reasoning in `MODEL_BENCH.md`.

## 🔴 Traps
⛔ **Every cargo invocation in a systemd unit**: `MemoryMax=4G`, `JOBS=1`, `DEV_DEBUG=0`. Disk 5.9G
(4G aborts). ⚠️ **`source scripts/fetch-onnxruntime.sh`** or the agent will not link. ⚠️ Verify a
bench picked up its corpus by reading its **`plan:`** line — that would have caught a retry that
silently used the default corpus. ⚠️ `pkill -f` matches paths in your own command line.
