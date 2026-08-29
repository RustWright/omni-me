# NEXT

**Next action: roadmap step 7 — bring imports current**, in a *fresh* context. The ledger import is
months stale; use the paisa process to get it import-ready, same for the Obsidian journal (user will
point to the current copy). Steps 8–9 are box wipe → Phase 6 OTA, each its own push.

**Blocking [USER] first:** one config edit in the private overlay — an API source's currency→account
map, spelled out in `omni-me-private/tasks.md` and its `credentials.toml.example`. Until it lands
that source imports nothing; startup warns loudly.

## Decisions in force — inherit these, don't re-derive

- **Email ingest is CUT from v1** (user, 2026-08-29); roadmap step 6 is struck. What was rejected is
  the ingest *model*: the gate needs a `watched_label` plus a `sender_patterns` list, config that is
  open-ended — it grows once per service the user signs up for. IMAP is off behind
  `OMNI_ENABLE_IMAP=1` at the private composition root (*not* by emptying the mailbox list — a stale
  `[imap.*]` entry must not silently re-enable it). Rationale in `tasks.md` step 6. **Auto-import
  itself stays a v1 requirement** — via APIs only. Do NOT re-enable IMAP to "fix the failing
  pollers": symptoms are recorded and they were never the problem.
- **`poppler-utils` no longer matters for v1** — `pdftotext` had exactly one caller, the email path.
  The Gemini header fix is still code-only and rides steps 8–9; deliberate, don't re-fix.
- **One roadmap push per fresh context.** Do the current step and stop. The pre-v1 code review is
  **CLOSED** (2026-08-28) — file new bugs as ordinary work, as the overlay bug was. `reviews/` is
  gitignored; `project.md`'s session-log rows are the durable record.

## Do NOT re-survey

- Whether an account editor belongs in `BatchReviewView`, or `verify()` wired into auto-import. Both
  planned, checked against the code, **deliberately not built** — API drafts carry a deterministic
  real account plus an intentional `Unmatched` mirror, and no auto-import source touches the
  extractor once IMAP is off. Anchors in `tasks.md` 2026-08-29. Also: the four Phase A review
  documents and their triage — every finding is dispositioned and dated.
- Formatting: public workspaces rustfmt-clean (CI-enforced); the private overlay has pre-existing drift in no gate — leave it.

## Open threads

- Neither API source is confirmed proposing drafts in production — one was silently misconfigured
  (now fixed, unverified against the live box), the other needs its helper binary locatable there or
  it is skipped with only a warning. Both tracked in `omni-me-private/tasks.md`. Known flake: a
  SurrealDB tempfile race in the suite, passes on rerun. `ui-checklist.md` is stale (deleted nav).
