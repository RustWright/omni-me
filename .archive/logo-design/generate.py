#!/usr/bin/env python3
"""Emit omni-me logo candidates as standalone 1024x1024 SVGs.

Each file is a full-bleed icon source (charcoal background + mark), the same
form `cargo tauri icon` consumes. Re-run after editing a mark below.
Brand: charcoal #1e1e1e, accent #448aff, off-white #dcddde.
"""
import os

OUT = os.path.dirname(os.path.abspath(__file__))
BG, BLUE, WHITE = "#1e1e1e", "#448aff", "#dcddde"

# Marks authored in a 0..100 coordinate space (center 50,50). %(B)s=blue, %(W)s=white.
MARKS = {
    "A1-omni-ring": '''  <circle cx="50" cy="50" r="28" fill="none" stroke="%(B)s" stroke-width="6"/>
  <circle cx="50" cy="50" r="9" fill="%(B)s"/>''',
    "A2-omni-eye": '''  <circle cx="50" cy="50" r="28" fill="none" stroke="%(B)s" stroke-width="6"/>
  <circle cx="57" cy="43" r="8.5" fill="%(B)s"/>''',
    "A3-aperture": '''  <circle cx="50" cy="50" r="28" fill="none" stroke="%(B)s" stroke-width="6"
          stroke-linecap="round" stroke-dasharray="154.4 21.5" transform="rotate(-55 50 50)"/>
  <circle cx="50" cy="50" r="9" fill="%(B)s"/>''',
    "B1-segmented": '''  <circle cx="50" cy="50" r="28" fill="none" stroke="%(B)s" stroke-width="6"
          stroke-linecap="round" stroke-dasharray="15.4 6.6"/>
  <circle cx="50" cy="50" r="8" fill="%(B)s"/>''',
    "B2-segmented-bold": '''  <circle cx="50" cy="50" r="27" fill="none" stroke="%(B)s" stroke-width="9"
          stroke-linecap="round" stroke-dasharray="25.4 8.5"/>
  <circle cx="50" cy="50" r="7" fill="%(W)s"/>''',
    "C2-orbit": '''  <ellipse cx="50" cy="50" rx="30" ry="11" fill="none" stroke="%(B)s" stroke-width="4"
           transform="rotate(-22 50 50)"/>
  <circle cx="50" cy="50" r="8.5" fill="%(B)s"/>
  <circle cx="68.7" cy="34.8" r="5" fill="%(B)s"/>''',
    "D-constellation": '''  <g stroke="%(B)s" stroke-width="2.5" opacity="0.85">
    <line x1="50" y1="50" x2="28" y2="34"/><line x1="50" y1="50" x2="72" y2="32"/>
    <line x1="50" y1="50" x2="68" y2="70"/><line x1="50" y1="50" x2="32" y2="66"/>
  </g>
  <circle cx="28" cy="34" r="5" fill="%(B)s"/><circle cx="72" cy="32" r="5" fill="%(B)s"/>
  <circle cx="68" cy="70" r="5" fill="%(B)s"/><circle cx="32" cy="66" r="5" fill="%(B)s"/>
  <circle cx="50" cy="50" r="8" fill="%(B)s"/>''',
}

TPL = '''<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 100 100">
  <rect width="100" height="100" fill="{bg}"/>
{mark}
</svg>
'''

sub = {"B": BLUE, "W": WHITE}
for name, mark in MARKS.items():
    svg = TPL.format(bg=BG, mark=(mark % sub))
    path = os.path.join(OUT, "candidate-%s.svg" % name)
    with open(path, "w") as f:
        f.write(svg)
    print("wrote", os.path.basename(path))
