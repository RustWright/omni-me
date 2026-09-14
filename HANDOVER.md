# Handover: what Claude needs before running dev-server and on-device testing alone

**Written 2026-09-13**, against the goal the user set on 2026-09-12: reach feature-complete so
Claude can *"run the dev server and on device testing independent of me and work on catching
and cleaning up all the bugs."*

The point of writing it now rather than at the handover is that some items need his hands, and
discovering that after feature-complete would waste the window the whole strategy protects.

Verify every claim here against live state before acting on it — this is a checklist, not a
snapshot.

---

## Already possible, no help needed

These were checked on 2026-09-13 and work today.

- **Full gate suite.** `cargo fmt` (both workspaces), clippy across all four configs, backend
  tests, frontend tests. ⚠️ `source scripts/fetch-onnxruntime.sh` first, after a leading `cd`
  into the repo — without it the `--features embeddings` link fails with an undefined C++
  symbol out of `ort_sys`, which looks nothing like a missing env var.
- **Browser UI iteration.** `dx serve --platform web --features mock --port 8080` driven by
  Playwright MCP. See `UI_WORKFLOW.md`.
- **APK builds.** `tauri-app/scripts/android-build.sh`. ⛔ Never `cargo tauri build` directly —
  it embeds whatever the debug dir last held, which is how a mock-data APK once shipped.
- **A second server instance.** ✅ Unblocked 2026-09-13: `OMNI_LISTEN_ADDR` now overrides the
  port, which was the last hardcoded piece. `DB_PATH` and `BLOB_DIR` were already relative, so
  the working directory is the isolation boundary. Seeding starts from
  `scripts/seed-bench-hub.py`.
- **`adb` itself.** Present at `~/android-sdk/platform-tools/adb`, not on `PATH` — call it by
  full path rather than concluding the toolchain is missing.

## Needs the user's hands

### 1. An Android device reachable over adb — the real blocker

`adb devices` returns an empty list. Nothing here is a software gap: authorising USB debugging
is a prompt **on the phone**, and no amount of host-side configuration substitutes for someone
tapping it.

What is needed, once:

- The dev phone connected, unlocked, with USB debugging on and this host authorised.
- Ideally `adb tcpip 5555` afterwards, so the connection survives the cable being unplugged and
  Claude can reconnect across sessions without another physical step.
- A decision on whether the phone stays connected between sessions or is plugged in per test
  run. Staying connected is what makes unattended on-device work possible at all.

⚠️ Screen-lock will interrupt an unattended run. Worth setting a long timeout, or staying
awake while charging, on the dev device only.

### 2. Where the dev sync server runs

The code no longer blocks a second instance, but the deployment choice is his:

- **Locally** — Claude can do this end to end today, no help needed. Good enough for everything
  except testing real cross-device sync.
- **On the box, beside the live server** — needs a second unit file in a second directory, and
  the deploy path goes through a private workflow dispatch. Whether Claude has the credentials
  for that is unverified; ⛔ do not test it by attempting a deploy.

Local is the recommendation until a test genuinely needs two devices talking to each other.

### 3. Real data, staged

Deferred on purpose and batched with on-device work — ⛔ not to be raised as a blocker before
then. Recorded here only so the eventual ask is not a surprise: the finance corpus needs
staging somewhere Claude can read it, and the user has said he will not point the app at his
live data for a first test.

### 4. Explicitly NOT wanted

- ⛔ **IMAP credentials.** `OMNI_ENABLE_IMAP` stays off and is never flipped without him.
  Enabling it today would archive all mail with drafting still on the old hardcoded sender
  patterns and no purge path.

---

## What is worth deciding before the handover, not during

- **Blast radius.** Which of these may Claude do unattended: restart the dev server, wipe and
  reseed the dev database, install an APK over the existing one, factory-reset app data?
  ⛔ Each is reversible only if the data is genuinely disposable, which is the property the
  separate dev instance exists to guarantee — but "disposable" is his call, not a derivation.
- **Failure policy.** On a test that fails in a way that needs a decision, does Claude stop and
  wait, or pick the reading that keeps the run going and report it? The second gets more done
  overnight and is the reason the autonomy is wanted; it is also how a wrong assumption
  compounds for six hours.
