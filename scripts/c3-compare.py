#!/usr/bin/env python3
"""Rank a C3 slate on the documents every model completed.

    scripts/c3-compare.py runs/20260915-185233-c3

Why this exists rather than reading the per-model scorecards: `rank_and_print`
sums recall over the documents that did **not** error, so each model's
denominator is its own. A model that refuses the hard 6-page document scores on
the five easy ones and looks better than one that attempts all six (R17). Each
bench process knows nothing of the others, so the correction cannot live there.

Two tables, deliberately:

  * the **common subset** — documents every model completed, which is the only
    apples-to-apples ranking;
  * **coverage** — how many documents each model refused, and which. A model
    that wins the common subset by refusing everything hard has not won, and
    that fact is invisible in the first table by construction.

Ranking order is `MODEL_THRESHOLDS.md`'s, lexicographic and never a weighted
sum: invented figures ascending, then figure recall, then word recall.
"""

import pathlib
import re
import sys

ROW = re.compile(
    r"^(doc-[0-9a-f]{4})\s+(\d+)\s+"
    r"(?:(\d+)/(\d+)\s+(\d+)/(\d+)\s+(\d+)\s+(\d+)ms|ERROR\s+(.*))$"
)


def is_structural(err):
    """Will this refusal repeat on the same document, every time?

    An image cap is a stated property of the endpoint: 6 pages will exceed a
    4-image limit on every attempt, so scoring it zero is simply true. A timeout
    or a rate limit is one bad attempt under whatever load the endpoint was
    carrying — it earns a retry before it is held against the weights.
    ⚠️ Conflating the two is R13's error (one error column, several unrelated
    causes) in a new place.

    ⛔ The test must mention images. An earlier version matched a bare
    `"too many"` and so classified **HTTP 429 Too Many Requests** — a rate limit,
    the most transient failure there is — as a permanent capability limit, which
    then counted against the model as unreadable documents.
    """
    e = err.lower()
    if "429" in e or "too many requests" in e:
        return False
    return "image" in e and ("too many" in e or "at most" in e)


def parse(path):
    """One model's rows: {tag: dict} plus the errors it hit."""
    done, errors = {}, {}
    for line in path.read_text().splitlines():
        line = re.sub(r"\x1b\[[0-9;]*m", "", line).rstrip()
        m = ROW.match(line)
        if not m:
            continue
        tag, pages = m.group(1), int(m.group(2))
        if m.group(9) is not None:
            errors[tag] = m.group(9).strip()
        else:
            done[tag] = {
                "pages": pages,
                "words_found": int(m.group(3)),
                "words_total": int(m.group(4)),
                "figs_found": int(m.group(5)),
                "figs_total": int(m.group(6)),
                "invented": int(m.group(7)),
                "ms": int(m.group(8)),
            }
    return done, errors


def main():
    if len(sys.argv) < 2:
        sys.exit(f"usage: {sys.argv[0]} <run-dir> [+extra-dir ...] [substring ...]")
    run = pathlib.Path(sys.argv[1])

    # Extra directories merge on top — that is how a retry pass folds in. A
    # later directory's row for the same document REPLACES the earlier one,
    # because the retry exists precisely to supersede a transient failure.
    extras = [pathlib.Path(a[1:]) for a in sys.argv[2:] if a.startswith("+")]
    # Bare arguments filter models by substring. ⚠️ Not cosmetic: eliminating a
    # model on the first ranking key and re-comparing the survivors is a real
    # step, because every model that fails often shrinks the intersection for
    # everyone else — Mistral and Qwen3-VL-30B cost the 2026-09-16 run-off six
    # of its sixteen common documents.
    keep = [a for a in sys.argv[2:] if not a.startswith("+")]

    logs = sorted(run.glob("*.log"))
    if not logs:
        sys.exit(f"no scorecards in {run}")

    models = {}
    for f in logs:
        if keep and not any(k.lower() in f.stem.lower() for k in keep):
            continue
        done, errors = parse(f)
        if done or errors:
            models[f.stem] = (done, errors)

    for extra in extras:
        for f in sorted(extra.glob("*.log")):
            if f.stem not in models:
                continue
            rdone, rerrors = parse(f)
            done, errors = models[f.stem]
            for tag, row in rdone.items():
                done[tag] = row          # a retried success supersedes
                errors.pop(tag, None)
            for tag, err in rerrors.items():
                if tag not in done:      # still failing after the retry
                    errors[tag] = err

    if not models:
        sys.exit("no parseable rows — did the slate produce scorecards?")

    # The intersection is over documents that COMPLETED everywhere. A document
    # every model refused is not in it either, which is correct: nobody was
    # measured on it.
    common = set.intersection(*(set(d) for d, _ in models.values()))
    every = set().union(*(set(d) | set(e) for d, e in models.values()))

    print(f"models: {len(models)}   documents attempted: {len(every)}   "
          f"completed by all: {len(common)}")
    if not common:
        print("\n⛔ NO COMMON SUBSET — no document was completed by every model, so "
              "there is no comparable ranking. Read the coverage table only.")
    print()

    print("COMMON SUBSET — the only comparable ranking")
    print(f"{'model':<46}{'invent':>8}{'figures':>20}{'words':>22}{'med s':>8}")
    print("─" * 104)
    rows = []
    for name, (done, _errors) in models.items():
        sub = [done[t] for t in common]
        inv = sum(r["invented"] for r in sub)
        ff = sum(r["figs_found"] for r in sub)
        ft = sum(r["figs_total"] for r in sub)
        wf = sum(r["words_found"] for r in sub)
        wt = sum(r["words_total"] for r in sub)
        ms = sorted(r["ms"] for r in sub)
        med = ms[len(ms) // 2] / 1000 if ms else 0
        # Lexicographic: invented ascending, then the two recalls descending.
        rows.append((inv, -(ff / ft if ft else 0), -(wf / wt if wt else 0),
                     name, inv, ff, ft, wf, wt, med))
    for r in sorted(rows):
        _, _, _, name, inv, ff, ft, wf, wt, med = r
        fr = f"{ff}/{ft} ({100*ff/ft:.1f}%)" if ft else "—"
        wr = f"{wf}/{wt} ({100*wf/wt:.1f}%)" if wt else "—"
        print(f"{name:<46}{inv:>8}{fr:>20}{wr:>22}{med:>8.1f}")

    print()
    print("PRODUCTION SCORING — a refusal the model will always repeat counts as zero")
    print("(structural = the endpoint's own limit; transient = one bad attempt)")
    print(f"{'model':<46}{'invent':>8}{'figures':>20}{'words':>22}{'struct':>8}")
    print("─" * 104)
    prod = []
    for name, (done, errors) in models.items():
        # Denominator is every document ATTEMPTED by anyone, so a refusal cannot
        # shrink the divisor. A document nobody could read is excluded, since it
        # separates no one.
        inv = sum(r["invented"] for r in done.values())
        ff = sum(r["figs_found"] for r in done.values())
        wf = sum(r["words_found"] for r in done.values())
        ft = wt = 0
        for tag in every:
            truth = next((m[0][tag] for m in models.values() if tag in m[0]), None)
            if truth is None:
                continue  # nobody read it; it is not evidence about anybody
            ft += truth["figs_total"]
            wt += truth["words_total"]
        nstruct = sum(1 for e in errors.values() if is_structural(e))
        prod.append((inv, -(ff / ft if ft else 0), -(wf / wt if wt else 0),
                     name, inv, ff, ft, wf, wt, nstruct))
    for r in sorted(prod):
        _, _, _, name, inv, ff, ft, wf, wt, nstruct = r
        fr = f"{ff}/{ft} ({100*ff/ft:.1f}%)" if ft else "—"
        wr = f"{wf}/{wt} ({100*wf/wt:.1f}%)" if wt else "—"
        print(f"{name:<46}{inv:>8}{fr:>20}{wr:>22}{nstruct:>8}")

    print()
    print("BY PAGE COUNT — does a capped model earn a primary/fallback split?")
    print(f"{'model':<46}{'≤4pp figures':>16}{'≤4pp words':>14}{'>4pp done':>11}")
    print("─" * 88)
    for name, (done, errors) in models.items():
        short = [r for r in done.values() if r["pages"] <= 4]
        longd = [r for r in done.values() if r["pages"] > 4]
        sff = sum(r["figs_found"] for r in short)
        sft = sum(r["figs_total"] for r in short)
        swf = sum(r["words_found"] for r in short)
        swt = sum(r["words_total"] for r in short)
        nlong_possible = len([t for t in every
                              if any(t in d and d[t]["pages"] > 4
                                     for d, _ in models.values())])
        f = f"{100*sff/sft:.1f}%" if sft else "—"
        w = f"{100*swf/swt:.1f}%" if swt else "—"
        print(f"{name:<46}{f:>16}{w:>14}{len(longd):>5}/{nlong_possible:<5}")

    print()
    print("COVERAGE — what each model refused, and whether it would repeat.")
    print(f"{'model':<46}{'done':>6}{'failed':>8}  reasons")
    print("─" * 92)
    for name, (done, errors) in sorted(models.items(),
                                       key=lambda kv: len(kv[1][1])):
        why = "; ".join(sorted({("[STRUCTURAL] " if is_structural(e) else
                                 "[transient] ") + e[:44]
                                for e in errors.values()})) or "—"
        print(f"{name:<46}{len(done):>6}{len(errors):>8}  {why}")


if __name__ == "__main__":
    main()
