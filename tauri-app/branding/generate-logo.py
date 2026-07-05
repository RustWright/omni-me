#!/usr/bin/env python3
"""Round 3 (v4) — refine the chosen ENSO 'om' for small-size legibility.

The enso was tapering to almost nothing right at its opening, so it dissolved at
32px. Fixes: fatten the thin end (less extreme taper), ROUND the brush terminals
(cap circles), and tighten the gap a touch. Three weights (light/medium/bold) so
the exact boldness is a pick, not a guess.

System: the 'o' alone = app icon; 'om' = compact wordmark (o + clean gateway m).
"""
import os, math
OUT = os.path.dirname(os.path.abspath(__file__))
B, W, BG, SW = "#448aff", "#dcddde", "#1e1e1e", 7
LETTER_CORE, ICON_CORE = 2.8, 5.0
LF = 14 / 27  # letter scale relative to the icon (R14 vs R27)

def core(cx, cy, r): return f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="{W}"/>'
def _pt(cx, cy, r, deg):
    a = math.radians(deg); return (cx + r*math.cos(a), cy + r*math.sin(a))
def _f(p): return f"{p[0]:.2f},{p[1]:.2f}"

def enso(cx, cy, R, ri, off, gapc, gaph):
    """Tapered brush crescent, opening centered at gapc (deg), with round caps."""
    cxi, cyi = cx + off, cy - off               # inner shifted toward the gap (top-right)
    a1, a2 = gapc + gaph, gapc - gaph            # gap edges
    o1, o2 = _pt(cx, cy, R, a1), _pt(cx, cy, R, a2)
    i1, i2 = _pt(cxi, cyi, ri, a1), _pt(cxi, cyi, ri, a2)
    d = f"M{_f(o1)} A{R} {R} 0 1 1 {_f(o2)} L{_f(i2)} A{ri} {ri} 0 1 0 {_f(i1)} Z"
    s = f'<path d="{d}" fill="{B}"/>'
    for a, b in ((o1, i1), (o2, i2)):            # round the two brush terminals
        mx, my, rr = (a[0]+b[0])/2, (a[1]+b[1])/2, math.dist(a, b)/2
        s += f'<circle cx="{mx:.2f}" cy="{my:.2f}" r="{rr:.2f}" fill="{B}"/>'
    return s

gateway_m = (f'<path d="M50,68 L50,52 A7.5 7.5 0 0 1 65,52 L65,68 M65,52 A7.5 7.5 0 0 1 80,52 L80,68" '
             f'fill="none" stroke="{B}" stroke-width="{SW}" stroke-linecap="round" stroke-linejoin="round"/>')

TPL = ('<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 100 100">\n'
       '  <rect width="100" height="100" fill="' + BG + '"/>\n{body}\n</svg>\n')

# weight dial (at icon scale R=27): smaller ri + smaller offset = bolder, less taper
WEIGHTS = {
    "a-light":  dict(ri=17, off=1.6, gaph=24),
    "b-medium": dict(ri=16, off=1.3, gaph=22),
    "c-bold":   dict(ri=15, off=1.0, gaph=20),
}
for name, p in WEIGHTS.items():
    icon_mark = enso(50, 50, 27, p["ri"], p["off"], -45, p["gaph"]) + core(50, 50, ICON_CORE)
    with open(os.path.join(OUT, f"icon-enso-{name}.svg"), "w") as f:
        f.write(TPL.format(body=f'  {icon_mark}'))
    letter = enso(30, 54, 14, p["ri"]*LF, p["off"]*LF, -45, p["gaph"]) + core(30, 54, LETTER_CORE)
    om = f'  <g transform="translate(1.5,-4.5)">\n    {letter}\n    {gateway_m}\n  </g>'
    with open(os.path.join(OUT, f"om-enso-{name}.svg"), "w") as f:
        f.write(TPL.format(body=om))
    print("wrote", name)
