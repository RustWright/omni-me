# Extraction integration test fixtures

Phase 2.8 (`core/tests/extraction_integration.rs`) expects representative
real-world samples here. All tests are `#[ignore]`d by default — they only
run when invoked explicitly with `--ignored`.

## Required files

| Filename         | Format                         | Used by hint           |
|------------------|--------------------------------|------------------------|
| `receipt.jpg`    | Phone photo of a paper receipt | `Receipt`              |
| `brokerage.pdf`  | Investment account statement   | `BrokerageStatement`   |
| `paystub.pdf`    | Payroll paystub                | `Paystub`              |
| `email.txt`      | Plain-text email body          | `EmailBody`            |

## Running

```bash
export GEMINI_API_KEY=$(cat ~/.config/omni-me/gemini-key)
cargo test -p omni-me-core --test extraction_integration -- --ignored --nocapture
```

`--nocapture` prints the extractor's per-test stderr output — useful for
eyeballing what Gemini actually returned without parsing the full
`raw_response`.

## What the tests check

Each test asserts only loose structural properties (postings non-empty, mix of
positive/negative signs for paystubs, etc.) — they're diagnostic against real
samples, not green-bar required. The unit tests in `core/src/extraction/`
cover the deterministic logic (parsing, verification, routing).
