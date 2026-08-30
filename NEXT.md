# NEXT

**Next action: nothing to build — step 7 waits on the handoffs below.** When the vault copy lands,
re-measure with `.reference/vault-dryrun-tool` and import; steps 8–9 follow, one push each.

## Blocked on [USER] — the only things left

1. **A session rooted at `~/Documents/paisa`** — ask for it explicitly. The request: download
   June/July/August statements for the ~9 active accounts, drop each where that importer's glob finds
   it, `regen_all.sh`, `paisa update`. Destinations in `~/Documents/paisa/NEXT.md`.
2. **The updated Obsidian vault.** pCloud is the delivery method only — the user makes a local copy and
   **says when it is ready**. Never read the mount live: they use the vault daily with sync off.
3. **Push this repo (40 commits) → push the overlay (3) → deploy → then the API source's config map.**
   Verified on the box 2026-08-30: its credentials file is untouched since 2026-06-28 and the running
   image `sha-869e707` == the overlay's `origin/main`, so the new key is unreadable and the config
   edit alone is a **no-op**. `deploy.yml` builds the overlay against *this* repo's pushed `main`, so
   this repo goes first. **The IMAP gate is undeployed too — the box is still polling.** Source- and
   institution-specific detail: `omni-me-private/tasks.md`.

## Decisions in force — inherit these, don't re-derive

- **Credentials live on the box** — `/etc/omni-me/credentials.toml` (`:ro`, read at boot) governs bank
  sources; client copy is `~/.config/omni-me/…`; both repos' `secrets/*.toml` are **templates**. Full
  table + the 2026-08-26 client-file trap: memory note `project_auto_import_host`.
- **The vault import design is settled in `omni-me-private/examples/headless_import.rs`**, the tool
  that did the real ingestion: filename-only (`classify_path`), frontmatter-`date:` fallback rejected,
  collisions demoted to Generic, `Templates/` skipped, bad-frontmatter notes rescued. The app's
  `scan_for_preview` differs on purpose — reasoning from *it* was the mistake struck 2026-08-29.
- **Email ingest is CUT from v1** (user, 2026-08-29) — the ingest *model*, not the pollers: the gate
  needs `watched_label` + `sender_patterns`, config that grows per service signed up for. IMAP is off
  behind `OMNI_ENABLE_IMAP=1` at the composition root. **Auto-import stays v1, via APIs only.**
- **`poppler-utils` is moot for v1**; the Gemini header fix rides steps 8–9. **One push per context.**

## Do NOT re-survey

- **The vault delta.** 2026-08-30, read-only: **49 new journal dates, `2026-07-04 .. 2026-08-22`**,
  pCloud a strict superset; generic 343 / demoted 4 / scan errors 19 / `Alton Hardin.md` rescue all
  **identical** to the 2026-06-22 baseline (1283/343/4/19) — no new data issues.
- Pre-v1 review **CLOSED**; account editor in `BatchReviewView`, `verify()`: **not built** on purpose.

## Open threads

- **Incoming:** a parent-repo session is designing long-term project tracking in omni-me and will leave
  a note here with the approach + interface. Builds **here, post-v1** — don't start it, don't reorder.
- WS source unconfirmed in production (`omni-me-private/tasks.md`). SurrealDB tempfile race (passes on
  rerun). `ui-checklist.md` stale. `MEMORY.md` past its load cap.
