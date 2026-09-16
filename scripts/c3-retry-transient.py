#!/usr/bin/env python3
"""Stage a per-model retry of the documents that failed *transiently*.

    scripts/c3-retry-transient.py <run-dir> <sample-dir> [retry-root]

Prints a `bench-c-slate.sh` invocation per model. Nothing is run from here — the
retry has to go through the same cage and unit as the original.

**Why retry at all.** The run-off scores a structural refusal as zero, because an
endpoint's image cap will refuse that document every time and the archive really
does get no text. A 429 or a 300s timeout is a different claim: it says the
endpoint was busy once. ⛔ Holding one bad minute against the weights is the same
conflation R13 made with a single error column.

**Why per-model directories.** Each model failed on a different set, so a shared
retry corpus would re-run documents that already succeeded — paying twice and,
worse, replacing a good result with a fresh draw from a flaky endpoint.

⚠️ The bench tags documents by an FNV-1a hash of the file stem, never the name,
so the tags in a scorecard have to be mapped back through the sample directory.
"""

import pathlib
import re
import sys

ANSI = re.compile(r"\x1b\[[0-9;]*m")
ERR_ROW = re.compile(r"^(doc-[0-9a-f]{4})\s+(\d+)\s+ERROR\s+(.*)$")


def tag_for(stem):
    """Mirror of `transcription_bench::tag_for` — FNV-1a, four hex digits."""
    h = 0xCBF2_9CE4_8422_2325
    for b in stem.encode():
        h ^= b
        h = (h * 0x100_0000_01B3) & 0xFFFF_FFFF_FFFF_FFFF
    return f"doc-{h & 0xFFFF:04x}"


def is_structural(err):
    """Identical rule to `c3-compare.py` — keep the two in step.

    ⛔ Must mention images. A bare "too many" also matches HTTP 429 *Too Many
    Requests*, which is the most transient failure there is.
    """
    e = err.lower()
    if "429" in e or "too many requests" in e:
        return False
    return "image" in e and ("too many" in e or "at most" in e)


def main():
    if len(sys.argv) < 3:
        sys.exit(f"usage: {sys.argv[0]} <run-dir> <sample-dir> [retry-root]")
    run = pathlib.Path(sys.argv[1])
    sample = pathlib.Path(sys.argv[2])
    root = pathlib.Path(sys.argv[3] if len(sys.argv) > 3 else ".c3-retry")

    by_tag = {tag_for(p.stem): p for p in sample.iterdir()}
    if not by_tag:
        sys.exit(f"no documents in {sample}")

    any_work = False
    for log in sorted(run.glob("*.log")):
        text = ANSI.sub("", log.read_text())
        transient = []
        for line in text.splitlines():
            m = ERR_ROW.match(line.rstrip())
            if m and not is_structural(m.group(3)):
                transient.append(m.group(1))
        if not transient:
            continue
        any_work = True

        # ⛔ Read the model id out of the log, never rebuild it from the file
        # name. The slate names files with `tr '/' '-'`, which is **lossy** —
        # `deepseek-ai/DeepSeek-V4.1-Flash` and `meta-llama/Llama-4-...` both
        # carry a hyphen in the vendor, so splitting on the first one yields
        # `deepseek/ai-...`, a model id that does not exist. It happened to work
        # for two of four names, which is how this would have reached a run.
        m = re.search(r"role C3 — transcription · model (\S+)", text)
        if not m:
            print(f"# ⛔ {log.stem}: no model id in the log; skipped")
            continue
        model = m.group(1)
        out = root / log.stem
        out.mkdir(parents=True, exist_ok=True)
        for link in out.iterdir():
            link.unlink()
        missing = []
        for tag in transient:
            src = by_tag.get(tag)
            if src is None:
                missing.append(tag)
                continue
            (out / src.name).symlink_to(src.resolve())

        print(f"# {log.stem}: {len(transient)} transient failure(s)")
        if missing:
            # Loud rather than silent: a tag that does not map back means the
            # sample directory is not the one the run used.
            print(f"#   ⛔ {len(missing)} tag(s) not found in {sample}: {missing}")
        # ⛔ ONE LINE per command, no backslash continuation. The first version
        # split this across two lines and the runner rejoined them with
        # `grep -A | paste`; `grep -A` emits `--` separators between match
        # groups, `paste` paired those blindly, and the whole sequence slid out
        # of phase — producing a run against a model literally named `--` and a
        # second run that lost its env and silently used the default corpus.
        # A line that needs no reassembly cannot be reassembled wrongly.
        print(f"env OMNI_BENCH_CORPUS={out.resolve()} "
              f"OMNI_BENCH_TRANSCRIBE_SAMPLE={len(transient)} "
              f"scripts/bench-c-slate.sh transcription {run}-retry {model}")

    if not any_work:
        print("no transient failures — nothing to retry")


if __name__ == "__main__":
    main()
