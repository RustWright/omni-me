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
against the document's own stated total. A mismatch halves the draft's confidence, and the
response carries the warning and a `needs_review` flag. (Until 2026-09-17 this ran only in the
extraction bench, so a capture showed the model's own confidence however wrong its lines were.)

A receipt draft also gains its other side, an `Unmatched` posting, so reconciliation can pair it
with the bank's record of the same purchase. That posting is the **printed total**, never the sum
of the line items: it has to equal what the bank charged. When the lines disagree with the total,
the draft stays unbalanced by the difference and saving it is refused, so the misread lines reach
a person instead of vanishing into a counter-leg sized to match them.

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

### When the check runs but compares nothing

There is a second blind spot, and it is worse than the consistent misreading because the report
looks clean rather than merely unremarkable. On an email carrying a single amount — a Netflix
charge, a phone bill, most subscription notices — the model returns that amount as a line item
*and* copies it into `total`. The check then adds up one number and compares it against itself.
It agrees, no warning is emitted, and the draft reads as arithmetically verified.

Two attempts to prompt this away both failed. The second said, verbatim, that `total` must be
null and that copying the amount in builds a check that cannot fail; the Netflix body still came
back with the total filled. Account paths and signs obeyed the same prompt, so this is not a
model ignoring instructions wholesale. It will not leave that field empty.

The fix is therefore not a third attempt at the wording. `VerificationReport` carries a
`TotalCheck` saying whether the comparison was independent at all: `Performed` when two or more
amounts were summed against a stated total, or when any comparison actually disagreed;
`Vacuous` when one amount was compared with itself; `NotRun` when no total was extracted. A
model-supplied counter leg collapses to one contributing amount too, so the balanced pair that
`already_balanced` recognises is vacuous by the same rule.

This changes no behaviour today. Nothing consumes the distinction, and an unchecked total
deliberately does not move confidence — a single-amount email has almost no arithmetic to get
wrong, and charging it for that would route the highest-volume email shape into manual review
for a risk that is largely theoretical. The point is what it prevents later. The next feature
that gates on verification — an auto-commit threshold, a confidence rule — would otherwise read
an empty warnings list as evidence the arithmetic was checked, and on this shape that inference
is false. Recording the distinction costs a field. Discovering it later costs a wrong booking.

## What counts as a receipt

A vendor does not send one email per purchase. It sends an order confirmation, then a notice that
an item was substituted or refunded, then one saying the order shipped, then one saying it was
delivered, then a request to rate the experience. Every one of those comes from the same address.
Claiming mail by sender and turning whatever arrives into a transaction therefore books a single
purchase several times over, and books a satisfaction survey as a purchase of nothing.

This is not a deduplication problem, which is the tempting reading. Two messages about one order
are not copies: an order that is updated between confirmation and delivery states two different
amounts, both correct when written. No key over message identity decides which is real.

So the extractor is asked two questions about the email *itself*, separately from what it reports:

- **`document_kind`** — receipt, order confirmation, order update, shipping notice, feedback
  request, marketing, or other. The instruction is to judge what the email *is*, not what it
  mentions, because a survey that repeats the order total is still a survey.
- **`order_ref`** — the vendor's own order number, copied as printed. This is the handle that
  ties a vendor's several messages to one purchase, and unlike a mail header it survives being
  forwarded.

Only a kind that can carry a charge proposes a draft. A feedback request proposes nothing.

Three decisions inside that are easy to get backwards:

**An absent kind still proposes.** A model that omits the field has not told you the message is
uninteresting. Dropping a real purchase is invisible and permanent; proposing one too many costs
a dismissal in a review queue that exists anyway. The gate fails open, deliberately.

**An unrecognised kind does not.** A label this build does not know maps to `other`, which does
not book, but the raw label is recorded rather than discarded so a new vendor phrasing is visible
instead of silently swallowed.

**A zero-amount draft is refused whatever its label.** This is the backstop for the label being
wrong rather than the message, and it needs no taxonomy to be right: a draft with no money in it
is never something to review.

Shipping notices and order updates deliberately still propose, even though they duplicate an
earlier confirmation. They restate the order total, and for some orders they are the only message
that arrives. Collapsing a vendor's several messages into one revisable proposal is the remaining
half of this, and it is unbuilt: it needs a rule for vendors that print no order number, and a
decision about what a revision may do to a batch that was already committed. Guessing either one
forces the other.

## Salvaging a partial extraction

`ExtractedPosting::amount` is a required `Decimal` parsed from a JSON string. That is the right
shape for a field no draft can do without, and it was brittle in a way that only showed up under
a real mailbox: on 2026-09-20 a royalty notification came back with `"amount": ""` on one line,
serde refused the value, and the **whole document** failed. Every posting the model had read
correctly went with it.

The lesson generalises past this one field. When the producer is a language model, every required
field is a liveness risk, because the schema is a request rather than a guarantee. The neighbouring
`total` is already `Option<Decimal>` for the same reason.

`parse_response` therefore sanitises the postings array before deserializing it. A line whose
amount is unusable is dropped on its own; a line whose amount arrived as a bare JSON number is
recovered, since that loses no information and is the likeliest remaining way to miss the schema.
Dropping one line rather than the document is safe here specifically because
`receipt_extraction_to_drafts` emits one self-balancing draft per posting — the survivors are
still individually balanced, so a salvage cannot leave a half-written entry behind.

Salvage is never silent. The count lands in `ExtractionResult::dropped_postings`, `verify` turns a
non-zero count into a warning and halves the effective confidence, and the original response is
kept in `raw_response` so what was discarded stays inspectable. A draft that is knowingly
incomplete must not arrive looking like a clean one.

### What this exposed about the email path

Chasing that failure turned up something larger than the failure. `verify` had exactly one caller
— the manual `/documents/extract` upload route. Drafts arriving from IMAP went extract → drafts →
review inbox with no arithmetic cross-check at all. The path running unattended every thirty
minutes had the weaker guarantee, and the path a human had just driven by hand had the stronger
one, which is backwards.

The receipt handler now verifies too. It passes `EmailBody` rather than `Receipt`, and `EmailBody`
cross-checks a total when one is present without treating a missing total as suspicious: a receipt
email frequently never prints a grand total, where a receipt or paystub document always does.
Penalising every such email would have produced a warning nobody reads.
