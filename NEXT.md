# NEXT

**Next action: step 7 waits on two data handoffs — nothing else blocks.** Everything unblockable from
this machine was done 2026-08-30. Steps 8–9 follow, one push each.

## Blocked on [USER] — data only

1. **A session rooted at `~/Documents/paisa`** — ask for it explicitly. The request: download
   June/July/August statements for the ~9 active accounts, drop each where that importer's glob finds
   it, `regen_all.sh`, `paisa update`. Destinations already in `~/Documents/paisa/NEXT.md`.
2. **The updated Obsidian vault.** pCloud is the delivery method only — the user makes a local copy and
   **says when it is ready**. Never read the mount live: they use the vault daily with sync off.

## Decisions in force — inherit these, don't re-derive

- **Credentials live on the box** — `/etc/omni-me/credentials.toml` (`:ro`, read **at boot**) governs
  the overlay's sources; both repos' `secrets/*.toml` are **templates**. Editing it needs `compose
  restart`; `up -d` is a no-op for a bind-mount change. Detail: memory `project_auto_import_host`.
- **Deploy ordering:** `deploy.yml` builds the overlay against the *public* repo's pushed ref, so this
  repo pushes **first**. Port 3000 binds to the tailnet IP, so `curl localhost:3000` on the box → 000.
- **The vault import design is settled in `omni-me-private/examples/headless_import.rs`**, the tool
  that did the real ingestion: filename-only (`classify_path`), frontmatter-`date:` fallback rejected,
  collisions demoted to Generic, `Templates/` skipped, bad-frontmatter notes rescued. The app's
  `scan_for_preview` differs on purpose — reasoning from *it* was the mistake struck 2026-08-29.
- **Email ingest is CUT from v1** (user, 2026-08-29) — the ingest *model*, not the pollers. Off behind
  `OMNI_ENABLE_IMAP=1`, and since the 2026-08-30 deploy that is true in production too (2 sources
  spawn, no IMAP). **Auto-import stays v1, via APIs only.**
- **One roadmap push per fresh context.** Pre-v1 review **CLOSED**; `poppler-utils` moot for v1.

## Do NOT re-survey

- **The vault delta.** 2026-08-30, read-only: **49 new journal dates, `2026-07-04 .. 2026-08-22`**,
  pCloud a strict superset; generic/demoted/scan-errors identical to the 2026-06-22 baseline
  (1283/343/4/19) — no new data issues.
- **CI needs generated artifacts before it compiles anything.** `tailwind.css` and `frontendDist` are
  both `dx` outputs, gitignored, required at *macro-expansion* time — so a missing one fails
  `cargo test`/clippy, not just the build. Fixed 2026-08-30; all 5 jobs green.

## Open threads

- The other API source ticked `events=0` — possibly nothing new, possibly its helper is unresolvable
  on the box. Confirm on a later tick; detail in `omni-me-private/tasks.md`.
- **Incoming:** a parent-repo session is designing long-term project tracking; builds **here, post-v1**.
- SurrealDB tempfile race (passes on rerun). `ui-checklist.md` stale. `MEMORY.md` past its load cap.
