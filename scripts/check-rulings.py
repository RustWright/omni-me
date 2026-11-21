#!/usr/bin/env python3
"""Fail when a `ruling N (RULINGS.md)` citation points at a number RULINGS.md no longer has.

Also fails on a number listed twice. Scans every tracked text file, so a cleanup that
drops a cited ruling cannot pass without changing the code that depends on it.
"""

import re
import subprocess
import sys

LISTED = re.compile(r"^- \*\*(\d+)\.\*\*", re.MULTILINE)
# Case-insensitive, and tolerant of a comment line break between the number and the file:
# "Ruling 39" opening a sentence, or "ruling 39" wrapped onto the next `//` line, still counts.
CITED = re.compile(r"ruling (\d+)\s+(?://[/!]?\s*)?\(RULINGS\.md\)", re.IGNORECASE)


def main() -> int:
    rulings = open("RULINGS.md", encoding="utf-8").read()
    numbers = [int(n) for n in LISTED.findall(rulings)]
    errors = [f"RULINGS.md: number {n} is listed twice" for n in sorted(set(numbers))
              if numbers.count(n) > 1]

    files = subprocess.run(["git", "ls-files"], capture_output=True, text=True,
                           check=True).stdout.split("\n")
    cited = 0
    for path in filter(None, files):
        if path == "RULINGS.md":
            continue
        try:
            text = open(path, encoding="utf-8").read()
        except (UnicodeDecodeError, IsADirectoryError, FileNotFoundError):
            continue
        for m in CITED.finditer(text):
            cited += 1
            if int(m.group(1)) not in numbers:
                line = text.count("\n", 0, m.start()) + 1
                errors.append(f"{path}:{line}: cites ruling {m.group(1)}, not in RULINGS.md")

    for e in errors:
        print(e, file=sys.stderr)
    print(f"{len(numbers)} rulings, {cited} citations, {len(errors)} errors")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
