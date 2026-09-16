# NEXT
✅ **Model selection is CLOSED.** All six model seats are chosen, wired into
`secrets/credentials.toml` as six independent role tables, and verified resolving at runtime.
Nothing is running. ⛔ Do not re-open a seat without a reason from production.
Seats, all verified resolving at runtime on **this machine** (`secrets/credentials.toml`):
**A** + **B** `DeepSeek-V4-Pro` · **C1** + **C3** `DeepSeek-V4.1-Flash` · **C2** `gemma-4-31B-it`
· **D** `DeepSeek-V4-Flash` · **E** is local, not a model choice.
🔴 **NOTHING since `sha-9a0b1dd` has run on ANY real device** — not the box, not mobile, not
desktop. That is the whole reason Stage 4 exists. ⛔ **Do not deploy to the live container and do
not edit `/etc/omni-me/credentials.toml`.** The live server keeps running `sha-9a0b1dd` with no
`[llm]` section, untouched. (I proposed exactly that deploy on 2026-09-16 and was corrected.)

**Next: Stage 4 — a dev-only instance beside the live one.** ✅ Box access works over the
tailnet (keyless SSH, passwordless sudo, docker group) — route is in memory, not here, because
this repo is public. Isolation is the point:
own directory · own `OMNI_LISTEN_ADDR` · own database seeded from a **clone** · own blob dir
copied separately (🔴 blob bytes do not sync) · **own credentials file** — the six role tables go
*there*, never in `/etc/omni-me/`. Then a dev phone repointed from Settings.
✅ Memory sizing settled (user); ⚠️ long-term growth is filed, not a blocker.
## ⛔ Inherit — do not re-derive
- ⚠️ **`DeepSeek-V4-Flash` and `DeepSeek-V4.1-Flash` are DIFFERENT models** — D takes the first,
  C1/C3 the second. All four ids validated against the endpoint (HTTP 200).
- ⚠️ **Two seats passed over a pre-registered rule, deliberately.** **B** and **C2** each had a
  *publish unmeasured* condition fire and were filled anyway (user); both sections record it.
- 🔴 **R26: sampling is uncontrolled.** `temperature`/`top_p`/`seed` are set nowhere in
  `core/src/`; C1 needed three runs before its numbers stopped moving (0 flips → 13 → 0). The
  bench sends `temperature: 0` via `bench-openrouter.sh` only. ⛔ D was measured WITH it and
  A/B/C without — never compare their cards.
- 🔴 **`runs/20260916-110711-ab-t0/` is NOT rankable** — rate-limited (81 backoffs on one row).
  A and B came from the 03:53 run, which had zero free-form errors.
- ⛔ **Seat B is benched on seat A's work** — `check_in.rs` raises one question a day about
  beliefs and no slate case resembles it. Its pick is the best answer available, not a measurement.
- ⛔ **Seat E is NOT decided** — one read verb of five, two record types of six.
- ✅ **Belief option 2 is role B's long-term shape** (user) — repeated independent evidence.
  ⛔ Needs a per-role turn budget and a second scheduled question first.
- ⛔ **Rank C1 with `c1-compare.py`, never the summary line** (R25); `OMNI_BENCH_SAMPLE`
  **redraws**. C2 needs no comparator (R24); C3 needs `c3-compare.py` (R17).
- ⛔ **Closed:** catalogue retrieval · blob store · archive page · `imap_real.rs` · Phases D–G ·
  finance propose · 413 hazard · enrichment · **model selection**.
- 🔴 ⛔ **Every cargo invocation in a systemd unit**: `MemoryMax=4G`, `JOBS=1`, `DEV_DEBUG=0`.
  Disk 5.8G. ⚠️ `source scripts/fetch-onnxruntime.sh` or it will not link.
