# NEXT

**Next action: work `tasks.md` § ▶ RUNBOOK, in order, starting at Stage 0.** It is the executable
plan for ③ model selection and every deferred validation pass, written 2026-09-14 for this
session. ⛔ **The design behind each stage is already decided — read the runbook and § Model
selection, do not re-derive them, and do not re-open the ground-truth question.**
→ **ORDER (user):** ① instrumentation ✅ → ② notify-and-route ✅ → ③ model selection → ④ dev server + on-device. ✅ **Both of ④'s hard blockers are CLEAR: the phone is connected and the corpus has arrived.** ⚠️ Model selection still comes first — a fair test cannot be designed while the system moves, and it has now stopped.

## ⛔ Stage 0 is not optional
The blast-radius grant — *"delete and modify and test to your hearts content"* — is **conditional on isolation**, and the live-DB clone makes the dev environment **indistinguishable from production from the inside**. `DB_PATH` is relative, so a wrong working directory is enough to point a wipe at the live server. ⛔ **Destructive tooling must assert its target self-identifies as dev before acting.** Configuration being right is not a safety mechanism; a refusal is. ⛔ The live box and his real phone are outside the grant, always.

## 🔴 NOTHING IS LOCKED IN — all four roles are re-evaluated (user, 2026-09-14)
⛔ **Role A is NOT decided.** `gpt-oss-120b-Turbo` may win again, but it is re-tested against the app as it is now, not inherited. ⚠️ `MODEL_BENCH.md` still reads "DECIDED 2026-09-10" for A — **historical, not in force.**

## ⛔ Decisions in force — inherit, do not re-open
- ✅ **FAILURE POLICY: stop and wait on a SIGNIFICANT failure** — and ⛔ **write down what was stopped for and what the run would have done next.** He recalibrates from that record (*"if I come back to a failure that would have been trivial I can change my mind"*), so a stop with no written reason wastes the stop. Flaky test or missing dev dependency = trivial; note it and continue.
- ✅ **"No real data" was always a WRITE rule** — *"there would not be junk entries made during testing that would have to be cleaned."* A clone satisfies it: junk lands on the clone, which is wiped and reseeded. ⛔ The constraint is **intact, not relaxed**; reading real data was never in its scope. ⚠️ **Lesson: do not widen a stated constraint past its direction — ask which way it points.**
- ⛔ **NO HAND-LABELLED GOLD SET** (*"I don't have the time"*). ✅ **The correction log already works unchanged** — a correction is a second `DocumentFieldsExtractedPayload` with `source: "human"` outranking `model:<name>@<ver>`, append-only, so both values persist. ⚠️ Labels are **negative-only**: nothing distinguishes "reviewed and accepted" from "never opened".
- ⛔ **Role C is THREE jobs**, split by what can check the answer: C1 arithmetic-checkable · C2 **no oracle by construction** · C3 **has no producer at all**. ⚠️ C2 and D share **one scorecard** — same criterion, different modality.
- ⛔ **No seat is decided on a saturated instrument.** Publish it unmeasured, fix the instrument, re-run. ⛔ Never break a tie by preference — that is the argued-not-measured choice the role table exists to prevent.
- ⛔ Thresholds and tie-break order written **before** each run; every instrument declares its noise floor and refuses to decide inside it (16 cases = 6 points of top-1).
- ⚠️ **Spending the OpenRouter credits is a GOAL, not a cost.** Screen **generously** on the gateway; keep only the **deciding run direct**. ⛔ Never trim the bench to save gateway spend. R11's ~60× cost gap is informational — *"the tests need to be done regardless"*.
- ✅ Parity confirmed for **text and latency only** (R1). ⚠️ **Vision parity is unproven** and Role C needs it — probe per role. ⛔ Re-resolve every model id (R2, case-sensitive, both halves); probe entitlement first (`-Ultra` is catalogued but **403** on this account).
- ⛔ **THE JOURNAL IS NEVER PROPOSABLE** · beliefs are ask-only · **irreversible actions can never be granted autonomy** · **`verified` means AN ORACLE CHECKED IT**; model-written fields are always `verified: false`.
- ⛔ **ARCHIVE IS THE SPINE** · **ONE EMAIL = 1 + N DOCUMENTS** · 🔴 **BLOB BYTES DO NOT SYNC** — one durable copy, the box; a clone must copy the blob dir separately, and a backup is verified by **size**, never exit code. ⛔ Server backup precedes the **real backfill**, not dev-server work.
- ⛔ **IMAP STAYS OFF**; never flip `OMNI_ENABLE_IMAP` without him.
- ⛔ **Do not re-survey, ALL CLOSED:** retrieval is catalogue-driven · blob store + LRU · the archive page and its three viewer bugs · `imap_real.rs` · Phases D–G · finance propose actions · **the whole archive plan, Phases 1–5** · Phase 5's notify-and-route · the role-wiring fix.
- ⛔ **`HANDOVER.md` is DELETED — do not recreate it.** Decisions here; runbook and inventory in `tasks.md`.

## ✅ Available now — verified 2026-09-14, not assumed
- **Phone connected and authorised** — `5431324d59563398  device  model:SM_G960W  transport_id:3`. ⚠️ `adb` lives at `~/android-sdk/platform-tools/adb`, **not on PATH**. It is on USB; run `adb tcpip 5555` early.
- **Corpus at `.reference/`** (gitignored, 239 MB): **276 CSVs, 765 PDFs** — five institutions, a dozen account kinds (chequing, credit card, brokerage, registered, multi-currency), plus tax notices. ⛔ **Institution names stay OUT of this repo**; read them off the directory listing. ✅ **Receipt PHOTOGRAPHS at `~/omni-spike-images/`** — 8 POC JPEGs, outside the repo; two are multi-page, so one document spanning two images is testable. Render PDF pages for volume.
- 🔴 **The 413 limit bites on the ENCODED payload, not the file.** `data:` URLs inflate 4/3: those photos are **2.8–3.8 MB on disk but 3.7–5.0 MB encoded**, and `receipt-4.jpg` crosses ~5 MB while looking 24% under. ⛔ Downscale first. ⚠️ **ImageMagick is NOT installed** — use PIL 9.0.1 or `ffmpeg`.
- **Both API keys present** — `[openrouter]` and `[llm]` in `secrets/credentials.toml`.
- ✅ **Role wiring landed**: `build_llm_client(creds, options, role)`. A ← interactive · B ← the scheduled check-in · C ← `build_extractor` · D ← `POST /notes/{id}/process`. ⚠️ Previously **every** role silently got role A's model. `scheduled: true` on the question payload is the discriminator.

## 🔴 Build traps that cost a session if forgotten
`JOBS=1`, `CARGO_PROFILE_DEV_DEBUG=0`, and ⛔ `source scripts/fetch-onnxruntime.sh` **after a leading `cd <repo>`** — without it any embeddings link dies on an undefined C++ symbol from `ort_sys` (`__isoc23_strtol`), which looks nothing like a missing env var. 🔴 **A FULL DISK PRESENTS AS A LINKER BUS ERROR.** ⚠️ A filter matching zero tests still prints `ok` — `-p omni-me-agent --lib` matches **nothing**, the agent is a binary crate and needs `--bins`. ⚠️ Frontend clippy without `--all-targets` does not build tests. ⛔ Build APKs with `tauri-app/scripts/android-build.sh`, never `cargo tauri build`. ⛔ Keep logs OUTSIDE the repo — SessionEnd pushes to a **PUBLIC** remote.
