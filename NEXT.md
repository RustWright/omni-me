# NEXT

**Next action: step 8 — wipe the box, then one clean re-import.** Step 7 is DONE: both handoffs
landed 2026-08-30, a dry-run into a throwaway DB passed all four gates. Only the write remains.

**Order is not the obvious one.** Wipe the box **first** (or keep the app closed until it is) — a
freshly wiped `local.db` re-pulls the box's old data on next open and undoes the clean slate. Then
move `local.db` aside (don't delete until the new one validates) → one `headless_import` →
`dump_balances` + `probe_realdb` → open the app and let it push. Recipe in `tasks.md` step 8;
assertions to reproduce in `omni-me-private/examples/README.md` § Expected output. Two traps:
**`OMNI_DEVICE_ID=$(cat <app_data>/device_id)` is not optional** (omitting it strands the import
behind a phantom id — the "absent on mobile" bug), and **`OMNI_VAULT` is the NESTED path** (the
directory name repeats; the outer level also holds a stray `Attachments/` with 2 orphan PDFs).

## Decisions in force — inherit these, don't re-derive

- **Vault import design is settled in `headless_import.rs`** — filename-only (`classify_path`),
  frontmatter-`date:` fallback rejected, collisions demoted to Generic, `Templates/` skipped,
  bad-frontmatter notes rescued. `scan_for_preview` differs on purpose (reasoning from it = the
  mistake struck 2026-08-29).
- **Idempotency is asymmetric** — financial skips on content-hash `txn_id`, journal UPSERTs by date,
  generic notes mint a fresh ULID and **duplicate all 343**. Hence the deferred write.
- **Credentials live on the box** (`/etc/omni-me/credentials.toml`, `:ro`, read at boot; editing
  needs `compose restart`) and **`deploy.yml` builds the overlay against the *public* pushed ref**,
  so this repo pushes first. Port 3000 binds to the tailnet IP — `curl localhost:3000` → 000.
- **Email ingest is CUT from v1** (the model, not the pollers); off behind `OMNI_ENABLE_IMAP=1`.
- **One roadmap push per fresh context.** Pre-v1 review **CLOSED**.

## Do NOT re-survey

- **The data.** Vault is a strict superset (+58 contiguous daily notes, 0 lost), other counts
  unchanged → no new issues; ledger clean through 2026-08-28 (`.reference/`).
- **`ledger bal --flat` is INCLUSIVE, the app EXCLUSIVE** — one account has both own postings and
  children, so a naive diff shows a spurious 2-row mismatch. Not a parser bug (private
  `examples/README.md` § Gotchas). Sitting-1's "verify before steps 7–8" Critical is **fixed**.

## Open threads

- **Two traps that silently no-op step 8** (memory `project-hetzner-db-reset-for-testing`): the wipe
  MUST pass `OMNI_VOLUME=omni-deploy_omni_data` (the default empties a stray volume and still reports
  success), and `push_local` is device-id-filtered — a clean single-id re-import is what avoids it.
  Test events are **discard, not sort** (user, 2026-08-14). Plus: SurrealDB tempfile race, stale
  `ui-checklist.md`, big `MEMORY.md`.
