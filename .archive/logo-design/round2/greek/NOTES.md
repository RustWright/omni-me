# Round 2 — Greek & Symbolic lane

Lane brief: Alpha/Omega, omega-as-vessel-holding-the-self, ouroboros, monad, enso,
vesica, and the "meaningful gap" (no app can fully encompass a life). Palette:
blue #448aff on charcoal #1e1e1e, off-white #dcddde as the two-tone focal.

All files: 1024x1024, `viewBox="0 0 100 100"`, charcoal rect is the first child,
mark kept inside ~radius 30 of center (50,50).

---

## gk-01-omega-vessel.svg  (drawn paths)
- **Concept:** A geometric Ω drawn as a horseshoe-arch that cups an off-white "me" dot.
- **Why it fits:** The literal seed the user loved — omega *holding the self*. Reads as the
  app's totality (omega = end/all) with the individual sheltered at the center. Two-tone depth.
- **Con:** The constructed omega with splayed feet risks looking like a plain horseshoe/magnet
  at 32px if the feet flatten out; the "Greek" read depends on the viewer knowing Ω.

## gk-02-ouroboros.svg  (drawn paths)
- **Concept:** A near-closed ring biting its own tail — bold head + off-white eye, thin tail,
  a small deliberate gap where mouth almost meets tail.
- **Why it fits:** The self-referential loop of logging your own life; the gap is the "meaningful
  gap" — the circle never fully closes because no system captures a whole life.
- **Con:** Uniform stroke can't truly taper head-to-tail, so the "serpent" read is subtle; at
  32px it may just look like an open ring with a thick dot.

## gk-03-alpha-omega.svg  (glyph / text)
- **Concept:** α and ω flanking an off-white self-dot — beginning and end, with "me" living between.
- **Why it fits:** Directly stages "the beginning and the end = totality" (omni), and puts the
  user at the center of their own span. Clean, literal, legible.
- **Con:** Three separate elements is busier than a single mark; α + ω spread wide means the
  outermost glyph edges sit near the circle-crop boundary and could clip slightly on Android.

## gk-04-monad-gap.svg  (drawn paths)
- **Concept:** Classical monad (point within a circle) with a clean gap cut at the top of the ring.
- **Why it fits:** Timeless "self at the center of the whole" symbol; the gap makes the wholeness
  honest/open (imperfect totality). Calm, iconic, reads at every size.
- **Con:** A ring-plus-dot is the most generic shape in this set — risks colliding with the
  "aperture/orbit" candidates from other lanes; leans on the gap alone to feel distinctive.

## gk-05-enso-brush.svg  (drawn paths)
- **Concept:** A single enso-style circle built as an offset-crescent so it tapers thick→thin into
  an opening at lower-right (brush-lift), with an off-white self-dot at center.
- **Why it fits:** Enso = wholeness + presence, and the open sweep carries the "meaningful gap"
  natively. The taper gives it a hand-made, un-AI, Obsidian-adjacent calm.
- **Con:** The taper is faked with two offset arcs; if a renderer mishandles the fill winding it
  could look lopsided, and the brush character is lost entirely at 32px.

## gk-06-omega-glyph.svg  (glyph / text)
- **Concept:** A clean, weighty Ω set in the brand font with an off-white dot resting in its cup.
- **Why it fits:** The most direct omega ↔ "omni" bridge and the production-typographic option —
  simplest possible identity mark. Dot keeps the "me" idea present.
- **Con:** Glyph rendering/vertical-centering varies by font and platform; needs outlining to paths
  before shipping, and without that the safe-zone centering isn't guaranteed.

## gk-07-vesica.svg  (drawn paths)
- **Concept:** Two overlapping circles forming a vesica; the almond lens is filled off-white with a
  blue self-dot at the exact intersection.
- **Why it fits:** Vesica = unity/overlap — the point where all ~13 domains meet is "me." Ancient,
  geometric, calm; the two-tone lens gives a strong focal without any glyph dependency.
- **Con:** Two full circles span wide, so the outer edges approach the circle-crop boundary; the
  symbol is more "sacred geometry" than obviously Greek, and can read as a Venn diagram.

## gk-08-omega-gate.svg  (drawn paths)
- **Concept:** Ω reimagined as a low architectural gateway/arch (round top, two posts, omega feet),
  with an off-white self-dot standing inside the portal.
- **Why it fits:** Same omega DNA but as a *threshold* — you pass through your whole life into the
  system; the self shelters within. Distinct, warmer, architectural rather than typographic.
- **Con:** Reads as "arch/doorway" first and "omega" second; the Greek meaning is the weakest of
  the omega set, so it may not carry the alpha/omega story on its own.
