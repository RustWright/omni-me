# PDF routing fixtures

Two PDFs of the **same fabricated receipt**, differing only in whether the page
carries a text layer. That difference is the whole fork `extraction::payload_for`
decides on, and the pair is what makes both arms of it testable.

All synthetic — the receipt is invented, per `core/tests/README.md`. These are
committed and run in CI (`core/tests/pdf_routing.rs`), unlike the real-sample
fixtures in `../extraction/`, which are `#[ignore]`d diagnostics.

| File                    | Text layer | Route it must take          |
|-------------------------|------------|-----------------------------|
| `generated-receipt.pdf` | yes        | `pdftotext -layout` → text  |
| `scanned-receipt.pdf`   | none       | `pdftoppm` → images         |
| `receipt.ps`            | —          | source both are built from  |

## Regenerating

Needs `ghostscript` (`ps2pdf`, `gs`), which is **not** a project dependency —
only fixture tooling. Poppler is the runtime dependency; ghostscript is only
what fabricates a believable scan.

```bash
cd core/tests/fixtures/pdf-routing
ps2pdf -sPAPERSIZE=letter receipt.ps generated-receipt.pdf
gs -q -dNOPAUSE -dBATCH -sDEVICE=pdfimage24 -r150 \
   -sOutputFile=scanned-receipt.pdf generated-receipt.pdf
```

`-sDEVICE=pdfimage24` re-renders every page to a 24-bit raster and wraps it back
into a PDF, discarding the text layer. That is what a flatbed scanner produces,
and it is why this is a real test of the scanned path rather than a stand-in:
`pdftotext` genuinely returns nothing for it.
