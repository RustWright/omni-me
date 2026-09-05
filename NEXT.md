# NEXT

**Next action: try it — Import statement → "Statement PDF", pick a file.** Needs a `[secrets]`
entry on the box for the password (none here — see threads). ✅ Part B done 2026-09-05: parser
(both layouts) → `POST /statements/parse` → Tauri command → one refusal gate shared by every
format. **136 real files: 0 failed self-checks, 0 replay findings**; counts match the old
system's exactly. Untried against a live box. **NOT next: generalizability.**

> **Public repo — fictional names only; real balances/institutions never enter it.** Institution
> detail lives in the overlay (`CONVENTIONS.md` is CANONICAL). The privacy guard is an **ingress
> filter, not an audit** — it sees only newly staged lines.

## Decisions in force — inherit these
- ⚠️ **NO LLM in the statement path** (user, 2026-09-05). Parsing is deterministic and
  self-checking. The overlay's mail handler still routes through `DocumentExtractor` — its doc
  says so loudly, and it's unreachable (IMAP off). **Classification** is deferred; intelligence
  belongs there.
- **Almost none of this is bank-specific** (user's question, 2026-09-05). Layout shapes and the
  checks generalize; only column *words*, date formats and the password rule don't — so the
  password is a **named `[secrets]` entry** (no new seam) and layout strings stay hardcoded.
- **A statement failing its own checks is NOT imported — every format** (user, 2026-09-05). One
  policy (`StatementParse::import_blockers`), one result type, one panel; `force` sits behind a
  read-the-failures button.
- ⚠️ **"Nothing failed" ≠ "verified."** The chequing export has no balance column, so it clears
  the gate by offering nothing to check; `Verifiability` says so in words. Never collapse it.
- **THE BAR: finance tab offline until import here beats the old system's** (user). Auto-import
  ticks are NOT the near path. Parity: overlay `IMPORT_PARITY.md`.
- ⚠️ **Ledger categories are NOT labels** — they're the OUTPUT of the rules under suspicion.
  **Amounts verified; categories by NOTHING.** Exit: overlay `CATEGORIZATION_DEFERRAL.md`.
  **No classifier without provenance. Sources stay OFF.**

## Do NOT re-survey
Parser rules validated against all 136 real files, twice. Statements: pCloud
`Backups/paisa-ledger/`; backlog `tasks.md`; `probe_realdb.rs`'s clippy lint predates you.

## Open threads
⚠️ **OOM-killed a shell this session** — 7.2GB RAM, 1.9GB swap: `CARGO_BUILD_JOBS=1`, one crate
at a time, never two cargo processes. · **No `credentials.toml` here**, so encrypted PDFs can't
be opened locally; the audit takes extracted `.txt`. · **Crypto modelled monthly — ask whether
intended.** · Institution not per-account · 3 need the **test phone** · memory prune owed.
