# NEXT

**Next action: try it — Import statement → "Statement PDF", pick a file.** Needs a `[secrets]`
entry on the box for the password (none here — see threads). ✅ Part B code-complete 2026-09-05:
parser (both layouts) → `POST /statements/parse` → Tauri command → UI refusal gate. **136 real
files: 0 failed self-checks, 0 replay findings**; counts match the old system's exactly. Untried
against a live box. **NOT next: generalizability.**

> **Public repo — fictional names only; real balances/institutions never enter it.** Institution
> detail is in the overlay (`ACCOUNT_MAPPING.md`, `SETUP.md`; `CONVENTIONS.md` is CANONICAL).
> The privacy guard is an **ingress filter, not an audit** — it sees only newly staged lines.

## Decisions in force — inherit these
- ⚠️ **NO LLM in the statement path** (user, 2026-09-05). Parsing is deterministic and
  self-checking. The overlay's mail handler still routes through `DocumentExtractor` — its doc
  says so loudly, and it's unreachable (IMAP off). **Classification** is deferred; that's where
  intelligence belongs.
- **Almost none of this is bank-specific** (user's question, 2026-09-05). Layout shapes and the
  declared-figure checks generalize; only column *words*, date formats and the password rule
  don't — so the password is a **named entry in the existing `[secrets]` map** (no new seam),
  and layout strings stay hardcoded pending the generalizability talk.
- **A statement failing its own checks is NOT imported** (document path); `force` sits behind a
  read-the-failures button. ⚠️ The **CSV path still imports then reports** — same class, left
  as-is deliberately; decide whether to unify.
- **An unavailable check is a FAILURE, not a pass** — hence declared opening/closing on the parse.
- **THE BAR: finance tab offline until import here beats the old system's** (user). Auto-import
  ticks are NOT the near path. Parity: overlay `IMPORT_PARITY.md`.
- ⚠️ **Ledger categories are NOT labels** — they're the OUTPUT of the rules under suspicion.
  **Amounts externally verified; categories by NOTHING. ABSTENTION over accuracy.** Exit:
  overlay `CATEGORIZATION_DEFERRAL.md`. ⚠️ **No classifier without provenance. Sources stay OFF.**

## Do NOT re-survey
Parser rules were validated against all 136 real files, twice. Statements: pCloud
`Backups/paisa-ledger/`; backlog: `tasks.md`. `probe_realdb.rs`'s clippy lint predates you.

## Open threads
⚠️ **OOM-killed a shell this session** — 7.2GB RAM, 1.9GB swap: `CARGO_BUILD_JOBS=1`, one crate
at a time, never two cargo processes at once. · **No `credentials.toml` here**, so encrypted PDFs
can't be opened locally; the audit takes extracted `.txt`. · **Crypto modelled monthly — ask
whether intended.** · Institution not per-account · 3 need the **test phone** · memory prune owed.
