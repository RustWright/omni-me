# Test data discipline

Parsers (journal import, statement CSV, bank adapters) are developed and tested
against **synthetic data that runs in CI**. Real financial data — a real
journal, a real statement, a real receipt — **never** enters a committed file.

The leak vector is not the gitignored database, it's the dev process: pasting a
real transaction into a test while chasing a parser bug. Fabricate a
representative synthetic example instead. Hold this before any parser work
against real data (Phase 4 go-live import).

## Coverage lives in synthetic tests, not in `#[ignore]`

Deterministic logic (the journal parser, the CSV mapper, the A2 rewriter) is
fully exercisable with fabricated input, so its coverage **must** come from
ordinary tests that run in CI — see the `TempDir`-based tests in
`src/journal_import.rs` and `src/auto_import/csv.rs`.

`#[ignore]` is **not** a home for logic coverage. A test that can never run in
CI gives zero regression protection and rots silently. Reserve `#[ignore]` for
the narrow case of a **real external resource you genuinely cannot synthesize**
— e.g. `tests/extraction_integration.rs` hits the real Gemini API, and "does the
model return parseable output for a real document" is not something a fixture
can fake. Those are manual *diagnostics*, run explicitly, and must skip
gracefully when their inputs/keys are absent.

Do **not** park a real-data test for deterministic logic. (A pre-cleanup
`journal_import_paisa.rs` did exactly this — a permanently-ignored at-scale run
against the real journal — and was deleted once we confirmed the synthetic CI
tests already covered every path it touched. Validating the parser against the
real journal at scale before go-live is a deliberate one-off run, not a parked
`#[test]`.)

## Where data lives

| Kind | Location | Committed? |
|------|----------|------------|
| Synthetic unit-test data | inline strings / `TempDir` in `#[cfg(test)]` | ✅ yes (runs in CI) |
| Synthetic integration fixtures | `core/tests/fixtures/**` | ✅ yes |
| **Real** journal / statements / receipts | `.reference/**` (e.g. `.reference/paisa/` = the real hledger journal, the Phase 4 import source) | ❌ **gitignored** |
| Runtime DB / blobs | `surreal_data/`, `blobs/` | ❌ gitignored |

## How it's enforced (mechanism, not vigilance)

- `.gitignore` covers `.reference/`, `surreal_data/`, `blobs/` — real data
  *physically cannot* be committed from these paths.
- The few legitimate `#[ignore]` real-resource diagnostics skip gracefully when
  their inputs are absent (return early with an `eprintln!` note), so CI — which
  never has them — stays green and a fresh public clone just sees them skip.

Run a real-resource diagnostic locally (when its inputs are present):

```bash
export GEMINI_API_KEY=$(cat ~/.config/omni-me/gemini-key)
cargo test -p omni-me-core --test extraction_integration -- --ignored --nocapture
```

See `fixtures/extraction/README.md` for the extraction sample inputs (kept local,
never committed).
