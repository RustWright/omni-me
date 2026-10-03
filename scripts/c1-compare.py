#!/usr/bin/env python3
"""Rank a C1 slate on pooled rates rather than the scorecard's summary line.

    scripts/c1-compare.py runs/20260916-021551-c1 [substring ...]

Why this exists rather than reading the summary each scorecard prints: that line
averages per-case percentages and formats with `{:.0}`, so eleven perfect cases
and one at 94.6% print as `mean recall 100%` — directly above the table showing
the 95%. It also drops errored cases from `labelled`. R25 in `MODEL_BENCH.md`
has the detail. The instrument is not fixed mid-slate, because changing it
between a slate's models makes the scorecards incomparable; this corrects the
reading instead, exactly as `c3-compare.py` does for C3's denominators.

Ranking order is `MODEL_THRESHOLDS.md` § Seat C1, lexicographic and never a
weighted sum: sign-flips, then misread figures, then recall, then REVIEW rate.

⛔ Key 5, cost per document, is NOT computed — the bench prints no token counts.
If the first four keys leave a tie, the tie-break is unavailable and the seat
publishes unmeasured rather than being decided on something else.
"""

import pathlib
import re
import sys

ANSI = re.compile(r"\x1b\[[0-9;]*m")

# CASE               DOC      LBL   RET  MATCH  SIGN   FAB  LATENCY  [error]
STATEMENT = re.compile(
    r"^(\S+/\S+)\s+(\S+)\s+(\d+)\s+(\d+)\s+(\d+)%\s+(\d+)\s+(\d+)\s+([\d.]+)s\s*(.*)$"
)

# DOC    HINT     PAGES  POST  TOTAL  ARITH  UNGRND  REVIEW  LATENCY  [error]
# The two enum columns make this unambiguous against any other line.
RECEIPT = re.compile(
    r"^(\S+)\s+(receipt|paystub|statement|brokerage|email|generic)\s+"
    r"(\d+)\s+(\d+)\s+(yes|no)\s+(ok|MISMATCH)\s+(\d+|n/a)\s+(FLAG|auto)\s+"
    r"([\d.]+)s\s*(.*)$"
)


def parse(path):
    statements, receipts = [], []
    for line in path.read_text().splitlines():
        line = ANSI.sub("", line).rstrip()
        m = STATEMENT.match(line)
        if m:
            statements.append({
                "case": m.group(1),
                "labelled": int(m.group(3)),
                "returned": int(m.group(4)),
                "flipped": int(m.group(6)),
                "fabricated": int(m.group(7)),
                "secs": float(m.group(8)),
                "error": m.group(9).strip() or None,
            })
            continue
        m = RECEIPT.match(line)
        if m:
            receipts.append({
                "doc": m.group(1),
                "pages": int(m.group(3)),
                "postings": int(m.group(4)),
                "arith_ok": m.group(6) == "ok",
                "review": m.group(8) == "FLAG",
                "secs": float(m.group(9)),
                "error": m.group(10).strip() or None,
            })
    return statements, receipts


def pooled(rows):
    """sum(returned) / sum(labelled) — the rate a per-case mean cannot give.

    Returned two ways on purpose. A model that errors on a case did not read it,
    and in production that is a missed statement, not an absent one; but the
    errored case may also be a transient endpoint failure rather than the
    weights. Reporting both keeps the choice visible instead of buried in a
    filter, which is the R17 mistake.
    """
    ok = [r for r in rows if r["error"] is None and r["labelled"] > 0]
    allr = [r for r in rows if r["labelled"] > 0]
    got_ok = sum(r["returned"] for r in ok)
    lbl_ok = sum(r["labelled"] for r in ok)
    lbl_all = sum(r["labelled"] for r in allr)
    return {
        "scored": got_ok / lbl_ok if lbl_ok else 0.0,
        "production": got_ok / lbl_all if lbl_all else 0.0,
        "labelled_scored": lbl_ok,
        "labelled_all": lbl_all,
        "returned": got_ok,
        "cases": len(allr),
        "errored": len(allr) - len(ok),
    }


def main():
    if len(sys.argv) < 2:
        sys.exit(f"usage: {sys.argv[0]} <run-dir> [substring ...]")
    run = pathlib.Path(sys.argv[1])
    keep = sys.argv[2:]

    models = {}
    for f in sorted(run.glob("*.log")):
        if keep and not any(k.lower() in f.stem.lower() for k in keep):
            continue
        s, r = parse(f)
        if s or r:
            models[f.stem] = (s, r)
    if not models:
        sys.exit(f"no parseable scorecards in {run}")

    print("STATEMENT ARM — pooled, not a mean of per-case rates")
    print(f"{'model':<50}{'flips':>7}{'misread':>9}{'recall':>18}"
          f"{'w/ errors':>12}{'err':>5}{'med s':>8}")
    print("─" * 109)
    rows = []
    for name, (s, _r) in models.items():
        p = pooled(s)
        flips = sum(x["flipped"] for x in s if x["error"] is None)
        fab = sum(x["fabricated"] for x in s if x["error"] is None)
        secs = sorted(x["secs"] for x in s if x["error"] is None)
        med = secs[len(secs) // 2] if secs else 0.0
        # Lexicographic per MODEL_THRESHOLDS: flips, misreads, then recall.
        rows.append((flips, fab, -p["scored"], name, p, flips, fab, med))
    for _, _, _, name, p, flips, fab, med in sorted(rows):
        rec = f"{p['returned']}/{p['labelled_scored']} ({100*p['scored']:.1f}%)"
        print(f"{name:<50}{flips:>7}{fab:>9}{rec:>18}"
              f"{100*p['production']:>11.1f}%{p['errored']:>5}{med:>8.1f}")

    print()
    print("GATE — MODEL_THRESHOLDS § Seat C1 requires MATCH >= 70% on this arm")
    for name, (s, _r) in models.items():
        p = pooled(s)
        verdict = "PASS" if p["scored"] >= 0.70 else "⛔ FAIL"
        print(f"  {name:<50}{100*p['scored']:>7.1f}%   {verdict}")

    if any(r for _s, r in models.values()):
        print()
        print("RECEIPT ARM — photographed documents, no text layer to check against")
        print("(so UNGRND is n/a by construction; this arm measures soundness, not truth)")
        # Split by page count. On the first scorecard of the 2026-09-16 slate
        # both unsound documents were the two-page ones and all four one-page
        # documents passed, which would point at the multi-part request path
        # (R17) rather than at reading a receipt. n=2 proves nothing; the column
        # exists so the slate's other models either repeat it or kill it.
        print(f"{'model':<50}{'docs':>6}{'sound':>7}{'1pp ok':>8}{'multi ok':>10}"
              f"{'review':>8}{'err':>5}{'med s':>8}")
        print("─" * 103)
        for name, (_s, r) in models.items():
            ok = [x for x in r if x["error"] is None]
            secs = sorted(x["secs"] for x in ok)
            med = secs[len(secs) // 2] if secs else 0.0
            sound = sum(1 for x in ok if x["arith_ok"])
            review = sum(1 for x in ok if x["review"])
            one = [x for x in ok if x["pages"] <= 1]
            many = [x for x in ok if x["pages"] > 1]
            one_s = f"{sum(1 for x in one if x['arith_ok'])}/{len(one)}"
            many_s = f"{sum(1 for x in many if x['arith_ok'])}/{len(many)}"
            print(f"{name:<50}{len(r):>6}{sound:>7}{one_s:>8}{many_s:>10}"
                  f"{review:>8}{len(r) - len(ok):>5}{med:>8.1f}")

    print()
    print("⛔ Key 5 (cost per document) is not computed — the bench prints no token")
    print("   counts. A tie surviving keys 1-4 publishes unmeasured.")


if __name__ == "__main__":
    main()
