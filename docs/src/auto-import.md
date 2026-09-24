# What counts as one purchase

> Status: **built 2026-09-23, not yet run against a live mailbox.** The grouping described here
> is exercised by unit tests over a real database in `core/src/events/auto_import_projection.rs`
> and `core/src/auto_import/receipts.rs`. The two decisions behind it were the user's, taken
> after a real Walmart order produced six review items.

omni-me can watch a mailbox, read what arrives and offer the transactions it finds. Nothing is
ever booked without a person accepting it, which is the subject of a different page. This one is
about a narrower question that turned out to be the hard part: a vendor sends several emails
about one purchase, and only some of them are a receipt.

A single order from one supermarket produced, in order: an order confirmation stating $105.43, an
"we've updated your order" restating $105.43, an "add more items to your delivery" stating
nothing, a delivery notice stating $99.93, and a satisfaction survey. Five messages, one
purchase, one amount that legitimately changed along the way. Committing what the importer
proposed would have booked roughly four times the money that left the account.

Two separate mechanisms are needed for that, and conflating them is what made the problem look
unsolvable. Some of those messages are not about money at all. The rest are about the *same*
money.

## The ones that are not a charge

The extractor classifies every message it reads — receipt, order confirmation, order update,
shipping notice, feedback request, marketing, other — and the importer proposes nothing for a
class that cannot carry a charge. A satisfaction survey is not a receipt no matter what figure a
model manages to find in it.

Two directions of that gate are deliberate and pull opposite ways.

An **unrecognised** label does not book. A new vendor phrasing lands in `Other`, which carries no
charge, and the label is still recorded verbatim so the phrasing is visible rather than lost.

An **absent** label still proposes. A model that omitted the field has not told us the message is
uninteresting, and a receipt whose amount failed to extract looks identical to a survey at this
point. Dropping it would lose a real purchase with nothing on screen to say so, where letting it
through costs one dismissal in a queue that exists anyway. The same asymmetry decides the rest of
this page: a duplicate is visible and costs a tap, a silently missing transaction is neither.

## The ones that are the same charge

Most real receipt emails print the vendor's own order number in the body. That is the handle,
and the importer keys the batch on it instead of on the message: every mail carrying
`118762884` lands on one review item, and the newest one wins the drafts.

**Not the `References:` header**, which was the first proposal and is wrong. A forward starts a
new thread, so a header-keyed group would have missed the forwarded copy of a receipt — which is
how these emails actually reach the system while it is being tested. A number printed in the body
survives being forwarded; a header does not.

A reference is only used when the message is charge-bearing and the reference itself is plausible:
four to sixty-four characters, no whitespace, and at least one digit after punctuation and case
are dropped. Those rules are not decoration. Asked for an order number on a message that has
none, models return the subject line, a bare `.`, or a UUID. Punctuation is dropped so a vendor
printing `#118-762-884` in one mail and `118762884` in the next still groups them.

### When there is no order number, nothing is grouped

The user's decision, 2026-09-23. A vendor that prints no reference keeps one review item per
message, as before.

The alternative was a fuzzy key — vendor plus date plus amount. It collides on two genuine
same-day purchases at the same merchant for the same amount, which is rare and real, and the
collision merges them into one item and loses a transaction. That failure is invisible. The
duplicate it would have prevented is not: it is sitting in the review queue, and dismissing it
costs a tap. So the cheap visible failure is chosen over the rare invisible one, and grouping is
added per class as reliable references are found rather than guessed at generally.

### A merge always shows what it merged

The order number comes from a model reading an email, so the key is good but not certain. The
mitigation is not a better key; it is refusing to make the merge invisible. The surviving review
item carries every proposal it displaced — the class, the subject, and the drafts verbatim — and
the review screen says so above the rows. A wrong merge therefore costs a reader who can see
$105.43 sitting under a $99.93 proposal, rather than a transaction that was never mentioned.

## A revision of something already committed

A shipping notice arriving after the confirmation was committed is the normal case, not an edge
case. It opens a **new review item** that names what it revises, and it does not touch the
committed transactions.

Amending them directly was the obvious-looking alternative and it crosses a line omni-me holds
everywhere else: committed books are the user's, and an unattended path that edits them is
exactly what the autonomy rules exist to forbid. Dropping the revision instead — on the grounds
that the order already has a transaction — was the cheaper fallback, and it fails the asymmetry
above: a refund or a price change would vanish.

So committing a revision **adds** transactions. Nothing already booked is changed or removed, and
the review screen says that in those words, because a reviewer who assumes otherwise will
double-book. Rows can be accepted individually, so the usual case is accepting the one line that
changed.

A message about a **dismissed** order is treated the same way, and that is a judgement rather
than a derivation. Dismissing the confirmation is not evidence that the receipt arriving an hour
later is unwanted — it is often the opposite, the confirmation having carried no usable total.
Re-dismissing costs a tap; the alternative loses the purchase.

Re-fetching the *same* message never produces any of this. Each proposal names the message it came
from, and re-polling a mailbox mints a fresh batch id for identical content, so the message
identity is the only thing that can tell a re-poll from a new mail. That distinction is what keeps
a dismissal dismissed while still letting the next real message through.

## What this does not do

- **Nothing here authenticates the sender.** There is no SPF or DKIM check, so anyone who knows
  the watched address can put text in front of the extractor. That is survivable only because
  every draft waits for a human commit, and it is the reason nothing on this path may ever
  auto-commit, however confident the extraction looks.
- **Arithmetic cannot catch a consistent misreading.** The verification pass compares line items
  against the document's own total, which catches a dropped tax line and cannot catch a receipt
  read coherently but wrongly.
- **The grouped row keeps the newest message's drafts.** If an earlier message was the better
  reading, that is visible in what it displaced, but correcting it means dismissing and entering
  the transaction by hand.
