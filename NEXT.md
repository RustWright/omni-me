# NEXT

**Next action: the finance work below — start with the paisa parity map** (user).
✅ `tasks.md` reconciled 2026-09-05 (685→286 lines, every item checked against code/git; resolved
work in `.archive/post-v1/`). Trust its `Reconciled against git` date, not its prose. Still queued:
a staleness signal for artifacts beyond `NEXT.md`. **NOT next: the generalizability conversation.**

> **Public repo — fictional names only; real balances/institutions never enter it.** The overlay
> has institution detail: `ACCOUNT_MAPPING.md` + `SETUP.md`; `CONVENTIONS.md` is CANONICAL.

## Decisions in force — inherit these
- **THE BAR: finance tab stays offline until statement import through omni-me meets and exceeds
  paisa's** (user). Auto-import ticks are NOT the near path. ⚠️ `core/src/statement/parse.rs` is
  **VERIFICATION ONLY** — the only importer is `parse_chequing_csv`, one format vs paisa's 7.
- **First move is a PARITY MAP, not code** — paisa's 7 importers
  (`~/pCloudDrive/Backups/paisa-ledger/importers/`) vs omni-me's 3 parsers: per format, what they
  handle that we don't. Lands in the overlay. **The user picks which ships first.**
- ⚠️ **The ledger is NOT labelled training data** — its categories are the OUTPUT of the rules
  under suspicion (~1% hand-pinned). Training on them learns the errors; evaluating certifies.
- **Trust boundary: amounts externally verified, categories by NOTHING** — balances/net-worth
  usable today; anything sliced by category or `kind` is not.
- **Audit the RULES, not the rows** — errors clump (one rule made all 162 mis-tags); 206 of 230
  fire, top 10 cover 64%. Cheap diagnostic, no ML: flag rules with *heterogeneous* matches.
- **ABSTENTION, not accuracy** — a rule confident by construction; the replay can't see it.
  **Solution space is OPEN, LLM not required**: rules+precedent → embeddings → classifier.
- **Deferral has a VERIFIED exit — overlay `CATEGORIZATION_DEFERRAL.md`** (read it before any
  classifier work): append `TransactionUpdated` with a full `postings` array — no schema change,
  no client update, sync-safe. ⚠️ **No classifier ships without posting provenance.**
- **BOTH bank sources stay OFF** — Push 2 fixed what they *emit*, not whether they run; Wise
  never ran a live tick. **Upstream blocker in paisa**: paystubs stop 2023-11-30 (~2.7y
  CPP/EI/tax, 66 PDFs) — **`Documents-paisa`.** Wise's fee-by-channel semantics live in `wise.rs`.

## Do NOT re-survey
Audit: **19 clean / 2 findings**, both the crypto question below. Statements live in pCloud
`Backups/paisa-ledger/` — symlink into one dir. Backlog reconciled: read `tasks.md`, not the logs.

## Open threads
**Crypto modelled monthly — ask whether intended**: 60/62 daily rows vs 4 monthly aggregates,
balances exact, counts differ. · Institution not remembered per account in the import form.
· 3 items need the **test phone** · Memory prune owed · format bar SHELVED.
