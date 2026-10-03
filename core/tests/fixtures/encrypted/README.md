# Encrypted PDF fixture

One genuinely encrypted PDF, user password `letmein`. It exists because 18.3% of
the real corpus (140 of 765 statements) is encrypted, and until 2026-09-27 every
one of them silently yielded no text: `archive.rs` passed an empty password and
`rasterize_pdf` accepted none at all, so the vision fallback could not open them
either.

All synthetic — the issuer and the figures are invented, per `core/tests/README.md`.
Committed and run in CI, like `../pdf-routing/`.

| File                       | Opens with | Proves                                        |
|----------------------------|------------|-----------------------------------------------|
| `encrypted-statement.pdf`  | `letmein`  | a wrong password is distinguishable from a damaged file |

## What it is for, precisely

Poppler reports **every** failure as exit 1 — measured against 24.02, and the man
page documents no code for encryption (only 0, 1, 2, 3 and 99). So the only thing
separating "none of your passwords opened this" from "this file is damaged" is the
stderr text:

| case                          | stderr                                  |
|-------------------------------|-----------------------------------------|
| wrong or absent password      | `Command Line Error: Incorrect password` |
| damaged or truncated file     | `Syntax Error: Couldn't find trailer dictionary` |

`statement::pdf::is_wrong_password` matches the first. A real encrypted file is
the only way to keep that match honest: a hand-written mock would assert what we
believe poppler prints rather than what it does.

## Regenerating

Needs nothing but python3 — the generator is beside the fixture because the
obvious tools are not project dependencies. `qpdf` and `reportlab` are both
absent from this box, and poppler, which *is* a dependency, only reads PDFs.

```bash
cd core/tests/fixtures/encrypted
python3 make-encrypted-pdf.py encrypted-statement.pdf letmein
```

The generator implements the standard security handler at revision 2 (RC4, 40-bit)
from the PDF spec: algorithm 3 for `/O`, algorithm 2 for the file key, algorithm 4
for `/U`, and algorithm 1 to key each object. That is the shape a bank's own
statement uses, and it is weak by modern standards — which is the point. The
fixture has to be openable by poppler, not secure.
