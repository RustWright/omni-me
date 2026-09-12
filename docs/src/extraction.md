# Reading documents

> Status: **built and run against a live endpoint, 2026-09-11.** The downscaling and
> rasterization described here replaced two gaps between what the provider trials established
> and what the code did. Both are exercised by unit tests in `core/src/extraction/media.rs` and
> confirmed against a real open-weights vision model on synthetic fixtures and on real receipts.
>
> ⚠️ The accuracy figures below come from a single afternoon against **one** model. They
> describe what the checks caught, not a measured error rate for the pipeline.

Photograph a receipt and omni-me turns it into a draft transaction you approve or discard. That
is one HTTP call from the app's point of view, and three quite different routes underneath,
because "document" covers a photo, a bank's generated PDF, and a scan of a piece of paper — and
no model reads all three.

## No model reads PDF

PDF is a container, not an image format. Every vision API that advertises PDF support converts
it first, somewhere you cannot see; OpenAI's `/chat/completions` — the shape every
open-weights server implements — does not offer that conversion at all. So omni-me does it,
and the route it picks depends on what is actually inside the file.

A **generated** PDF, which is what a bank statement is, already contains its text. It goes
through `pdftotext -layout`, and the `-layout` flag is the whole contract: a statement's
columns carry meaning, and reading-order text collapses every amount into the same position.

A **scanned** PDF contains no text at all — each page is a photograph of a page. Until recently
omni-me stopped here and said so. Now those pages are rendered to images with `pdftoppm` and
sent as pictures, which is the only thing that can work.

The order matters and is not interchangeable. A generated PDF *could* be rasterized and read
visually, and it would be worse: the model would guess at digits it could have had verbatim.
Rasterization is the fallback, reached only when the text pass comes back empty.

Both tools ship in `poppler-utils`, which is a system dependency of any host running the
server. It is the same dependency the statement importer already needed.

## Everything gets resized, because the limit is real

Images travel to the model inline, base64-encoded into the request body as `data:` URLs. This
is what lets omni-me work without a signed-URL subsystem or any public blob host — nothing has
to be reachable from the internet for the model to see it.

The cost is that the image counts against the request size, and requests are rejected above
roughly 5 MB. A photo from a modern phone is 3 to 8 MB before encoding, and base64 inflates it
by a further third. Left alone, the single most common thing anyone would do with this feature
fails.

So anything wider or taller than 2048 pixels is scaled to fit inside that box and re-encoded as
JPEG. Beyond 2048 pixels a vision model gains nothing — it is working from a fixed grid of
image tokens either way — so this costs no accuracy.

Three details are worth stating because they are not obvious:

**An image that already fits is forwarded untouched.** A cropped 900×1200 receipt is not
re-encoded, because a re-encode would soften the fine strokes of a faded thermal print, which
is exactly the text being read.

**Size is checked separately from dimensions.** A lossless screenshot can be under 2048 pixels
on both edges and still be far too large. Fitting the pixel box is not evidence of fitting the
byte budget, so both are checked.

**Transparency is composited onto white.** JPEG has no alpha channel, and simply dropping it
renders the transparent background of a screenshotted receipt black, hiding the text underneath.

## What is refused, and why refusing is the right answer

A scanned PDF of more than eight pages is rejected rather than partially read.

This is deliberate and it is the more useful half of the design. A truncated extraction of a
statement is a transaction list with rows silently missing — and it does not look like a
failure. It looks like a complete answer, gets approved, and the missing rows surface much
later as a balance that does not reconcile. An error you see immediately is cheaper than a
wrong number you trust.

Within that page limit, a set of pages that lands slightly over the request budget is retried at
progressively lower JPEG quality before being refused. The ladder is three steps and then it
stops; a quality search that could always find room would eventually ship an unreadable page
rather than admit the document is too big.

## The arithmetic check, and the one thing it cannot see

Every extraction is cross-checked before you see it: the line items are added up and compared
against the document's own stated total. A mismatch halves the draft's confidence and routes it
into a "look at this carefully" lane rather than letting it auto-commit.

This is the strongest signal available, because it does not depend on the model being honest
about its own uncertainty. A model that misreads 5.50 as 5.00 will still tell you it is 95%
confident. The receipt's own total will not agree with it.

Three real receipts run through this on 2026-09-11 were each caught: one omitted a tax line, one
read a duplicated item once, one read a single figure twice. In every case the draft said 95%
confident and the arithmetic said otherwise.

**What it cannot catch is a consistent misreading.** A fourth receipt reported the *subtotal* as
its total and listed only the pre-tax line items. Those two agree perfectly — the check passes,
confidence stays at 95%, and the draft is short by exactly the tax. The failure is invisible to
an arithmetic test precisely because the arithmetic is internally consistent.

The defence against that is not a better check, it is an unambiguous instruction: the extractor
now names which figure on a receipt is the total (the amount actually paid, after tax, never the
subtotal) rather than asking for "the total" and letting a document that prints three of them
decide. An ambiguous instruction does not produce noisy output. It produces confidently
self-consistent output, which is the harder kind to notice.

That receipt now extracts correctly — four items summing to the subtotal, both tax lines, and a
total equal to what was actually charged. Worth noting what the intermediate attempt cost,
though, because it is the same lesson twice: a first version of the instruction said to emit
each tax line as its own posting, and a receipt with one tax line came back with it listed three
times. Telling a model to be thorough about a category invites it to find more of that category
than exists. The wording that worked names what a line item **is** — an individual charge, never
a subtotal or a running balance, never the same amount twice unless the receipt says so.
