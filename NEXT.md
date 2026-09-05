# NEXT

**Next action: B3, the upload path — it opens with a DECISION, not code** (see Open threads).
✅ 2026-09-05: the third statement parser landed, both layouts, proven over the whole corpus —
**136 files, 0 failed self-checks, 76 clean / 60 with no activity, 0 replay findings**; counts
match the old system's exactly, and the 41 older-layout files it silently dropped now parse.
Audit: overlay `cargo run --example statement_audit`. **NOT next: generalizability.**

> **Public repo — fictional names only; real balances/institutions never enter it.** The overlay
> has institution detail: `ACCOUNT_MAPPING.md` + `SETUP.md`; `CONVENTIONS.md` is CANONICAL.
> The privacy guard is an **ingress filter, not an audit** — it sees only newly staged lines.

## Decisions in force — inherit these
- ⚠️ **NO LLM in the statement path** (user, 2026-09-05). Parsing is deterministic and now
  self-checking; `DocumentExtractor`/Gemini is queued for re-evaluation *as part of* the LLM
  push, and the overlay's mail handler still routes through it — its doc now says so loudly.
  **Classification** is where intelligence belongs and is already deferred.
- **Statements arrive by manual upload, not email.** IMAP stays off at the composition root.
- **An unavailable check is a FAILURE, not a pass** — hence declared opening/closing figures
  on the parse: without them a last row sitting mid-group reads as unverifiable.
- **THE BAR: finance tab stays offline until statement import through omni-me meets and exceeds**
  the old system's (user). Auto-import ticks are NOT the near path. Parity: overlay `IMPORT_PARITY.md`.
- ⚠️ **The ledger is NOT labelled training data** — its categories are the OUTPUT of the rules
  under suspicion (~1% hand-pinned). Training on them learns the errors; evaluating certifies.
- **Amounts externally verified; categories by NOTHING. ABSTENTION over accuracy** — space is OPEN.
- **Deferral has a VERIFIED exit — overlay `CATEGORIZATION_DEFERRAL.md`.** ⚠️ **No classifier
  without provenance.** **BOTH bank sources stay OFF.**

## Do NOT re-survey
Parser rules were validated against all 136 real files before being written, and again after.
Statements: pCloud `Backups/paisa-ledger/`; backlog: `tasks.md`, not the logs. `probe_realdb.rs`'s
clippy `type_complexity` is pre-existing, untouched, not yours.

## Open threads
**B3's fork:** the server has NO route-extension seam (`RunConfig` carries only
`source_builder`), so "decrypt server-side" needs one added — ask before building it.
· **No `credentials.toml` on this machine**, so encrypted files can't be opened here; the audit
takes already-extracted `.txt` for that reason. · **Crypto modelled monthly — ask whether
intended** (60/62 daily rows vs 4 monthly, balances exact). · Institution not remembered per
account in the import form · 3 need the **test phone** · Memory prune owed.
