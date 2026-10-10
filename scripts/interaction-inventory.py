#!/usr/bin/env python3
"""Every tappable thing in the frontend, grouped by page and component.

Reads the Dioxus `rsx!` source, so it needs no build and no device. For each
`onclick:` / `on_*:` handler it records the component it sits in, the first
label after it, and what the handler does:

  NAV   switches tab (`request_nav`)        VIEW  changes screen inside a page
  CALL  reaches the backend (`invoke_*`)    STATE changes local state only

It is a regex pass over source, not a parser: a label can be wrong where an
element's text comes from a variable, and a handler that delegates to a named
closure shows that closure's name, not its effect. The output is a map to read
alongside `interaction-map/README.md`, not a test.

Usage: scripts/interaction-inventory.py > interaction-map/inventory.md
"""
import collections
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent / "tauri-app/frontend/src"
FILES = sorted((ROOT / "pages").glob("*.rs")) + [
    ROOT / "main.rs",
    ROOT / "components/nav.rs",
    ROOT / "components/proposal_card.rs",
    ROOT / "components/attachment_viewer.rs",
    ROOT / "components/feedback.rs",
]
EFFECTS = [
    (r"request_nav|NavTarget", "NAV"),
    (r"\bview\.set\(|View::|FinancesView::|on_open\b|on_back\b|on_select\b|on_done\b", "VIEW"),
    (r"bridge::invoke_(\w+)", "CALL"),
    (r"\.set\(|\+= 1|toggle\(\)", "STATE"),
]


def handler_text(src: str, start: int) -> str:
    """The handler expression: up to the comma or bracket that closes it."""
    depth = 0
    for j, c in enumerate(src[start : start + 800]):
        if c in "({[":
            depth += 1
        elif c in ")}]":
            if depth == 0:
                return src[start : start + j]
            depth -= 1
        elif c == "," and depth == 0:
            return src[start : start + j]
    return src[start : start + 800]


def rows_for(path: pathlib.Path):
    src = path.read_text().split("#[cfg(test)]")[0]
    comps = [(m.start(), m.group(1)) for m in re.finditer(r"\n(?:pub(?:\(crate\))? )?fn ([A-Z]\w*)\(", src)]
    for m in re.finditer(r"\b(onclick|onsubmit|onchange|on_[a-z_]+)\s*:", src):
        if "EventHandler" in src[m.end() : m.end() + 40]:
            continue  # a prop declaration, not a use
        comp = next((n for s, n in reversed(comps) if s < m.start()), "?")
        handler = " ".join(handler_text(src, m.end()).split())
        after = src[m.end() + len(handler) : m.end() + len(handler) + 700]
        label = first_label(after)
        effects = set()
        for pattern, name in EFFECTS:
            for hit in re.findall(pattern, handler):
                effects.add(f"{name} {hit}" if name == "CALL" else name)
        yield comp, m.group(1), label, sorted(effects)


ATTRIBUTE = re.compile(r"\b(class|key|r#type|value|placeholder|id|href|src|style|accept|inputmode|role)\s*:\s*(format!\()?$")
TAILWIND = re.compile(r"(^|\s)[a-z]+(-[a-z0-9/\[\].%]+)+(\s|$)")


def first_label(text: str) -> str:
    """The first string literal that reads as words rather than an attribute value."""
    for m in re.finditer(r'"((?:[^"\\\n]|\\.)*)"', text):
        value = m.group(1)
        if not value.strip() or ATTRIBUTE.search(text[: m.start()].rstrip()):
            continue
        if TAILWIND.search(value) or not re.search(r"[A-Za-z]", value):
            continue
        return value[:60].replace("|", "\\|")
    return ""


def main():
    print("# Interaction inventory (generated)\n")
    print("Regenerate with `scripts/interaction-inventory.py > interaction-map/inventory.md`.")
    print("What the columns mean, and where this is wrong: the script's header.\n")
    total = 0
    for path in FILES:
        by_comp = collections.defaultdict(list)
        for comp, attr, label, effects in rows_for(path):
            by_comp[comp].append((attr, label, effects))
        if not by_comp:
            continue
        print(f"## `{path.relative_to(ROOT)}`\n")
        for comp, rows in by_comp.items():
            print(f"### {comp}\n")
            print("| handler | label nearby | effect |\n|---|---|---|")
            for attr, label, effects in rows:
                label = label.replace("|", "\\|")
                print(f"| `{attr}` | {label} | {', '.join(effects) or '—'} |")
                total += 1
            print()
    print(f"_{total} handlers._")


if __name__ == "__main__":
    main()
