# NEXT

✅ **The dev phone runs the app against the dev server and holds the full cloned history**
(15,957 events projected, 0 failed). Model selection and Stage 4 are closed.

▶ **NEXT ACTION: deploy the paginated-pull server image to the dev instance, then rebuild the
APK with the matching client loop.** The fix is written and clippy-clean; tests were still
compiling. Procedure and both workflow IDs are in the private overlay's `DEV_TESTING.md`.

## ▶ SCOPE — agreed with the user 2026-09-16
**Goal: a fully functional app, ending with his FINANCES TAB re-enabled** — the finish line, not
a checklist. ⛔ Test **every** feature, one step at a time.

🔴 **I drive the first pass; HE drives the final pass.** His reasoning governs the run: early
obvious bugs would sour his judgement before the feedback that matters. ⛔ Don't hand him a build
to evaluate until the crashes are out.

⛔ **Read `tasks.md` § DEFINITION OF READY** — the finish line is a *chain*: ledger catch-up (or
wipe + re-import) → IMAP receipt ingestion → receipt capture → archive → finance propose → tab
on. 🔴 Documents are on the critical path, NOT the tail. 🔴 **IMAP testing conflicts with the dev
isolation as built** — resolve before that leg.

✅ **Dev data is disposable** — reseed from the snapshot when it gets too polluted.

## ⛔ Inherit — do not re-derive
- 🔴 **NOTHING since the stamped release has run on live.** ⛔ Never deploy current work there,
  never edit the live `credentials.toml`. ⛔ `/var/omni-updates` is the LIVE OTA store.
- 🔴 **The box's `restore-snapshot.sh` targets the RUNNING container — i.e. LIVE.** Name the dev
  volume explicitly; `lib-guard.sh` is the fix and is not deployed.
- ✅ **`feature.finances = false` is the real synced value** — the tab is off by config, and that
  flag is what the chain ends by flipping. Its absence from the nav is correct, not a bug.
- ✅ **Dev APKs now carry `OMNI_WEBVIEW_DEBUG=1`** — drive the live page over CDP rather than
  tapping screenshots. ⛔ Never set it in `app-release.yml`; `src-tauri/build.rs` says why.
- ⚠️ **A token change needs an app restart** — background sync builds its client at startup.
  Documented, not a bug; the Settings toast says so.
- ⚠️ **Dev credentials carry no IMAP/bank sections** — a source there would poll real mailboxes.
- ⚠️ **Dev talks DIRECTLY to DeepInfra**, not OpenRouter.
- ⛔ **Branches:** public `dev/role-split-model-seats`, private `dev/untested-overlay`.
- 🔴 ⛔ **Every cargo invocation in a systemd unit**: `MemoryMax=4G`, `JOBS=1`. Laptop disk 3.5G.
