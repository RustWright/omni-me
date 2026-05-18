# Android live-device testing checklist

**Purpose:** track what to verify when the test Android device is plugged in. This is a temporary doc for the current testing turn — delete when the round is done and findings are folded into `project.md` / known-gaps.

**Build command:** `cargo tauri android dev` (cold-start the day-of testing; have `adb logcat | grep -i omni` ready in a side terminal)

**Server target:** ensure `OMNI_ME_SERVER_URL` or equivalent points at the Tailscale-reachable dev server (the phone has to reach the Hetzner-eventual / dev VPS for sync + extract endpoints).

---

## Phase 3.1 — Photo capture (camera + file picker)

- [ ] Tapping the Photo tile opens the system file picker (or camera with `capture=environment` hint on first launch)
- [ ] Selecting a photo from gallery → Working spinner appears
- [ ] Spinner resolves → ConfirmDraft form pre-populates with Gemini's extraction
- [ ] Edit-and-save records a `TransactionRecorded` event (verify via server logs or transaction count)
- [ ] Attachment cache mirror appears at `<app_data>/attachments/<sha256>` (rooted shell or adb pull)
- [ ] Attachment indicator visible on ConfirmDraft (filename + size line)
- [ ] Network failure mid-extract surfaces in `CaptureState::Error` with Retry button
- [ ] Retry button re-fires with same bytes (no re-pick required)

## Phase 3.2 — PDF capture

- [ ] PDF tile opens file picker (no camera prompt)
- [ ] PDF picker shows the hint selector chip row (bank/brokerage/paystub/receipt)
- [ ] Selected hint is reflected in the Gemini extraction (e.g., paystub hint → wage postings)
- [ ] Encrypted PDF fails extraction cleanly (no crash; error banner)

## Phase 3.3 — Android share-target intent

**This is the headline Cycle 3 mobile flow — verify aggressively.**

- [ ] Omni-Me appears in the share sheet of **Gallery** when sharing an image
- [ ] Omni-Me appears in the share sheet of **Gmail** when sharing an attached PDF
- [ ] Omni-Me appears in the share sheet of **Drive** when sharing a PDF
- [ ] Omni-Me appears in the share sheet of **Files** / **DocumentsUI** when sharing octet-stream files
- [ ] Share fires while app is **cold-starting** (intent caught in `onCreate`) — bytes survive
- [ ] Share fires while app is **already foreground** (intent caught in `onNewIntent`) — bytes survive
- [ ] Share fires while app is **background** then resumed — bytes still drained on next mount
- [ ] FinancesPage auto-switches in (other tabs get bypassed)
- [ ] "Use shared file" panel shows filename + size + MIME correctly
- [ ] Tapping "Use shared file" runs the same extraction flow as the file picker
- [ ] **Back from share-target view** clears the pending_share signal (doesn't re-route on next mount)
- [ ] Unsupported MIME (text/html share) silently drops to Home with no error popup
- [ ] PDF share preserves the hint picker affordance
- [ ] Two share intents in quick succession: second one replaces first; only the second is consumed
- [ ] Sharing the **same file twice** doesn't double-stash (Kotlin overwrites bytes file)
- [ ] App removal + reinstall: share sheet entry survives (intent-filter is in real manifest, not just gen/)

## Phase 3.4 — Email body paste

- [ ] Paste a real receipt email body into the textarea
- [ ] Extract button enables only when body is non-empty
- [ ] Extraction completes; draft populates
- [ ] Multiline paste preserves newlines (no auto-trimming that breaks regex on Gemini's end)

## Phase 3.5 + 3.6 — Manual entry + Confirm-draft

- [ ] Manual tile opens an empty form with two posting rows
- [ ] Adding a third posting row works
- [ ] Removing a row when only 2 remain is disabled
- [ ] Save fails cleanly when date is empty (no silent submit)
- [ ] Save fails cleanly when description is empty
- [ ] Save fails cleanly when amount is non-numeric
- [ ] Save with fewer than 2 valid postings surfaces "need at least 2" error
- [ ] Successful save closes form and returns to Home
- [ ] AttachmentRef indicator survives the Manual → ConfirmDraft route (extracted draft pre-population)

## Phase 3.7 — Attachment cache

- [ ] First save after extraction: bytes land in `<app_data>/attachments/<sha256>`
- [ ] Cache size reflected in Settings → Cache section
- [ ] Clear Cache button removes all files but preserves the directory
- [ ] Re-fetch after clear: cache repopulates from `/blobs/<sha256>` (slower; verify via timing or server log)
- [ ] LRU eviction kicks in past 200MB (synthesize by capturing many large attachments)

## Phase 3.8 — Settings → Cache section

- [ ] Cache size shown as human-readable (KB/MB)
- [ ] Clear button shows confirmation feedback
- [ ] After clear, displayed size drops to 0

## Phase 3.9 — Settings → Auto-Import sources

- [ ] All 5 sources listed (Wise, WS, IMAP receipts, IMAP SC NGN, … verify exact set)
- [ ] Healthy / Stale / Unknown / Degraded badges render with correct colors
- [ ] Tap "Fetch now" triggers a tick; banner shows result
- [ ] Error from a degraded source (e.g., wrong password) surfaces inline on the row
- [ ] Last-tick timestamp updates after a successful manual fetch
- [ ] Interval display matches configured per-source intervals

## Phase 3.10 — Auto-import batch pipeline (backend only this round)

**Scope this testing round:** UI deferred to next session — verify the backend pipeline indirectly via server logs + direct SurrealDB queries against the client. Anything UI-dependent gets `[deferred]`.

**Useful queries on the client's SurrealDB** (via `surreal sql` or whatever shell is convenient):
```
SELECT batch_id, source, dedup_key, status, fetched_at
FROM pending_auto_import_batches
ORDER BY fetched_at DESC;
```

- [ ] Trigger a manual Wise fetch (Settings → Auto-Import → Fetch now). Server log shows `tracing::info!` for "wise auto-import" with a non-zero draft count; the event log gains an `auto_import_batch_proposed` event
- [ ] Wait for sync; the client's `pending_auto_import_batches` table shows a new pending row with `source = "wise"` and the right draft_postings JSON
- [ ] Trigger an SC NGN fetch (or wait for natural IMAP poll). Same pipeline; `source = "sc_ngn"`; `dedup_key` shape is `sc_ngn-uid-{N}`
- [ ] **Idempotency check (IMAP-driven):** trigger a second SC NGN fetch within the polling window so the same message-UID gets re-fetched. Verify the projection still has only ONE pending row (UPSERT collapsing on same dedup_key)
- [ ] **Idempotency edge case:** if you can simulate (or wait for) a polling-source double-tick of Wise with no new transactions, expect TWO rows (different per-tick dedup_keys for polling sources — by design until Cycle 4 polish)
- [ ] Verify auto-import does NOT silently land `TransactionRecorded` events anymore — checking the `transactions` SurrealDB table after a tick should show NO new rows from auto-import (those only land after user commit, which is 3.10.5 UI work)
- [ ] **(deferred — 3.10.6 UI)** pending banner on Finances Home shows count
- [ ] **(deferred — 3.10.6 UI)** batch review screen renders per-row accept checkboxes + edit
- [ ] **(deferred — 3.10.6 UI)** NGN batches surface the FX prompt at the batch level
- [ ] **(deferred — 3.10.6 UI)** Save → `TransactionRecorded` × N + optional `ExchangeRateRecorded` land in the event log + projection
- [ ] **(deferred — 3.10.6 UI)** Dismiss → `AutoImportBatchDismissed` event lands; the next IMAP fetch of the same UID does NOT re-propose

---

## Cross-cutting / smoke

- [ ] App launches cold without crash
- [ ] Tab switching is responsive (Journal → Notes → Routines → Finances → Settings)
- [ ] Sync chip in header reflects real connection state
- [ ] Network airplane-mode + recovery → events queue locally and flush on reconnect
- [ ] Battery / process killed mid-extraction: no orphan attachment, no half-committed event

## Logging / observability while testing

- [ ] `adb logcat` shows MainActivity "share intent capture failed" messages on bad shares (e.g., empty stream)
- [ ] Server logs show `/documents/extract` requests with attach=true when expected
- [ ] Auto-import scheduler tick logs surface per-source (look for `handler = …` from sc_ngn.rs:178)

---

## Findings log

(record observed bugs/quirks here as you test — each becomes either a hotfix or a Cycle 4 backlog item)

| Date | Finding | Disposition |
|---|---|---|
| | | |
