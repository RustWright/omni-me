# NEXT

**Next action: Part B — the third statement parser** (plan `lets-continue-mellow-bunny.md`; the
institution is named in the overlay's `IMPORT_PARITY.md`, never here).
✅ Part A landed 2026-09-05: all statement imports now run through `core::statement::parse`, which
accounts for every line it reads. ✅ `tasks.md` reconciled 2026-09-05 (685→286 lines; resolved
work in `.archive/post-v1/`) — trust its `Reconciled against git` date, not its prose.
**NOT next: the generalizability conversation** — immediately before the LLM push only.

> **Public repo — fictional names only; real balances/institutions never enter it.** The overlay
> has institution detail: `ACCOUNT_MAPPING.md` + `SETUP.md`; `CONVENTIONS.md` is CANONICAL.
> The pre-commit privacy guard is an **ingress filter, not an audit** — it only sees newly staged
> lines, so moving old text into a new file trips it. Scan the whole tree to actually check.

## Decisions in force — inherit these
- ⚠️ **NO LLM in the statement path** (user, 2026-09-05). `DocumentExtractor`/Gemini is itself
  queued for re-evaluation *as part of* the LLM push; routing SC through it starts that push
  through a side door. Parsing is deterministic; **classification** is where intelligence belongs
  and is already deferred. Do not reintroduce it.
- **SC arrives by manual upload, not email.** IMAP ingest stays off at the composition root; the
  open-ended "which mail is relevant" objection never reopens.
- **THE BAR: finance tab stays offline until statement import through omni-me meets and exceeds
  paisa's** (user). Auto-import ticks are NOT the near path. Parity map: overlay `IMPORT_PARITY.md`.
- ⚠️ **The ledger is NOT labelled training data** — its categories are the OUTPUT of the rules
  under suspicion (~1% hand-pinned). Training on them learns the errors; evaluating certifies.
- **Trust boundary: amounts externally verified, categories by NOTHING** — balances/net-worth
  usable today; anything sliced by category or `kind` is not.
- **ABSTENTION, not accuracy** — a rule confident by construction; the replay can't see it.
  **Solution space is OPEN, LLM not required**: rules+precedent → embeddings → classifier.
- **Deferral has a VERIFIED exit — overlay `CATEGORIZATION_DEFERRAL.md`**: append
  `TransactionUpdated` with a full `postings` array. ⚠️ **No classifier without provenance.**
- **BOTH bank sources stay OFF.** Wise never ran a live tick. **Upstream blocker in paisa**:
  paystubs stop 2023-11-30 (~2.7y CPP/EI/tax, 66 PDFs) — **`Documents-paisa`**.

## Do NOT re-survey
Audit: **19 clean / 2 findings**, both the crypto question below. Statements live in pCloud
`Backups/paisa-ledger/`. Backlog reconciled: read `tasks.md`, not the logs.

## Open threads
**Crypto modelled monthly — ask whether intended**: 60/62 daily rows vs 4 monthly aggregates,
balances exact, counts differ. · Institution not remembered per account in the import form.
· 3 items need the **test phone** · Memory prune owed · format bar SHELVED.
