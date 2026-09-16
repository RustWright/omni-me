# NEXT
✅ **Stage 4 DONE — the dev instance is live and isolated.** Model selection is closed (six seats,
`MODEL_THRESHOLDS.md`). ⏳ A dev APK is building; it lands on the box at `~/omni-dev/apk/`.

**The full dev-vs-live layout is in `tasks.md` § Stage 4** — ports, volumes, credentials, the
phone's wireless adb, and where the APK lands. ⛔ Read it there; do not re-derive it by poking
at the box.
## ▶ SCOPE — agreed with the user 2026-09-16
**Goal: a fully functional app, ending with his FINANCES TAB re-enabled** — the finish line, not
a checklist. ⛔ Test **every** feature, **one step at a time**; expect many issues.

🔴 **I drive the first pass; HE drives the final pass.** His reasoning governs the run: early
obvious bugs would sour his judgement before the feedback that matters. ⛔ Don't hand him a build
to evaluate until the crashes are out. My pass finds and fixes; his judges.

⛔ **Read `tasks.md` § DEFINITION OF READY first** — the finish line is a *chain*, not a toggle:
ledger catch-up (or a full wipe + re-import) → IMAP receipt ingestion → receipt image capture →
archive → finance propose → tab on. 🔴 **Documents are on the critical path, NOT the tail** — an
earlier plan had them last and that was wrong. 🔴 **IMAP testing conflicts with the dev isolation
as built** (dev has no `[imap.*]` on purpose; a poller marks REAL messages processed) — resolve
before that leg. ⚠️ Storage capacity is an open worry the user raised himself; blobs read 0 today
only because nothing has been archived.

✅ **Dev data is disposable** — reseed from the snapshot whenever it is too polluted. That is the
rollback point, agreed.
## ⛔ Inherit — do not re-derive
- 🔴 **NOTHING since the stamped release has run on a real device.** ⛔ Never deploy current work
  to live, never edit `/etc/omni-me/credentials.toml`.
- 🔴 **The box's `restore-snapshot.sh` targets the RUNNING container — i.e. LIVE.** ⛔ Name the
  dev volume explicitly; `lib-guard.sh` is the fix and is not deployed.
- ⛔ **`/var/omni-updates` is the LIVE OTA store** — never publish the dev APK there.
- ⚠️ **`app-release.yml` bakes `:3000` into the APK** and its `publish` job cannot be skipped —
  hence `build-dev-apk.yml`: one job, dev port, no publish, no artifact upload (**Actions storage
  is exhausted**; passing files between jobs needs it). ⚠️ Both had `ubuntu-latest` break the
  Android SDK step when it moved 22.04→24.04; fixed with `packages: ''`, release path unverified.
- ⚠️ **Dev credentials carry no IMAP/bank sections** — a source there would poll real mailboxes.
- ⚠️ **Dev talks DIRECTLY to DeepInfra**, not OpenRouter — C1/C2/C3 were benched that way; A/B/D
  rely on the recorded gateway/direct text-parity finding (R1).
- ⛔ **Branches, not `main`:** public `dev/role-split-model-seats`, private `dev/untested-overlay`.
- 🔴 ⛔ **Every cargo invocation in a systemd unit**: `MemoryMax=4G`, `JOBS=1`. Laptop disk
  **4.4G** — an Android build will not fit locally, hence CI.
