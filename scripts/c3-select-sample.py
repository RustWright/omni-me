#!/usr/bin/env python3
"""Choose a page-stratified C3 run-off sample and stage it as symlinks.

    scripts/c3-select-sample.py [out-dir] [n]

Then point the bench at it:

    OMNI_BENCH_CORPUS=<out-dir> OMNI_BENCH_TRANSCRIBE_SAMPLE=<n> \
      scripts/bench-c-slate.sh transcription runs/... <models...>

**Why symlinks rather than a flag on the bench.** The selection has to be fixed
and reproducible before the run-off, and changing `transcription_bench.rs`'s
sampling in the middle of model selection is instrument tuning — the thing
`MODEL_THRESHOLDS.md` exists to prevent. A directory the bench walks normally
changes nothing about the instrument.

**Why stratify.** `born_digital` takes the first `want` PDFs in sorted order, and
the default 6 turned out to be **17%** over-4-pages against a corpus that is
**36.5%** — so the sample understated the endpoint image cap (R17), the very
thing separating the finalists, by half. It also drew 3 of 24 issuers. This
mirrors the corpus page distribution and spreads issuers within each bucket.

**Why nothing over 8 pages.** `MAX_DOCUMENT_PARTS = 8` refuses those before any
endpoint sees them, so every model fails them identically and they separate
nobody. 16 documents (2.6%) are excluded on that basis, and that exclusion is a
stated limit rather than a silent one.

Seeded, so re-running reproduces the same sample.
"""

import collections
import hashlib
import pathlib
import random
import subprocess
import sys

SEED = 20260915
CORPUS = pathlib.Path(".reference/paisa-ledger")
MAX_PAGES = 8


def page_count(path):
    """Pages, or None if the file cannot be read (encrypted, corrupt)."""
    out = subprocess.run(
        ["pdfinfo", str(path)], capture_output=True, text=True, timeout=20
    )
    for line in out.stdout.splitlines():
        if line.startswith("Pages:"):
            return int(line.split()[1])
    return None


def has_text_layer(path):
    """The oracle's precondition — `born_digital` uses the same 200-char floor."""
    out = subprocess.run(
        ["pdftotext", "-layout", str(path), "-"],
        capture_output=True,
        text=True,
        timeout=30,
    )
    return len(out.stdout.strip()) > 200


def issuer(path):
    """Hashed account directory. Never the name — these are institutions."""
    return hashlib.md5(path.relative_to(CORPUS).parts[0].encode()).hexdigest()[:4]


def main():
    out = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".c3-sample")
    want = int(sys.argv[2]) if len(sys.argv) > 2 else 30

    print(f"scanning {CORPUS} ...", file=sys.stderr)
    by_pages = collections.defaultdict(list)
    for p in sorted(CORPUS.rglob("*.pdf")):
        n = page_count(p)
        if n is None or n > MAX_PAGES:
            continue
        if has_text_layer(p):
            by_pages[n].append(p)

    universe = sum(len(v) for v in by_pages.values())
    if not universe:
        sys.exit("no eligible documents found")

    # Proportional allocation, largest-remainder so the parts sum to `want`
    # exactly rather than drifting by a rounding error per bucket.
    exact = {n: len(v) * want / universe for n, v in by_pages.items()}
    alloc = {n: int(v) for n, v in exact.items()}
    for n, _ in sorted(exact.items(), key=lambda kv: -(kv[1] - int(kv[1]))):
        if sum(alloc.values()) >= want:
            break
        alloc[n] += 1

    rng = random.Random(SEED)
    chosen = []
    for n, k in sorted(alloc.items()):
        pool = by_pages[n]
        # Spread issuers: shuffle, then prefer one document per issuer before
        # taking a second from any. A bucket drawn from one institution would
        # measure that institution's layout, not the page count.
        rng.shuffle(pool)
        seen, first_pass, rest = set(), [], []
        for p in pool:
            (first_pass if issuer(p) not in seen else rest).append(p)
            seen.add(issuer(p))
        chosen.extend((first_pass + rest)[:k])

    out.mkdir(parents=True, exist_ok=True)
    for link in out.iterdir():
        link.unlink()
    for p in chosen:
        # Keep the original stem so `tag_for`'s hash matches the main corpus and
        # the two runs' scorecards can be compared document by document.
        (out / p.name).symlink_to(p.resolve())

    dist = collections.Counter(page_count(p) for p in chosen)
    short = sum(v for k, v in dist.items() if k <= 4)
    print(f"\neligible universe: {universe} born-digital PDFs of ≤{MAX_PAGES} pages")
    print(f"selected: {len(chosen)} → {out}")
    print(f"  pages: {dict(sorted(dist.items()))}")
    print(f"  ≤4 pages: {short} ({100*short/len(chosen):.0f}%)   "
          f">4: {len(chosen)-short} ({100*(len(chosen)-short)/len(chosen):.0f}%)")
    print(f"  issuers represented: {len({issuer(p) for p in chosen})}")
    print(f"  corpus for comparison: 63.5% ≤4 pages, 36.5% over")


if __name__ == "__main__":
    main()
