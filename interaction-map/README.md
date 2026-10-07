# Interaction map

Every screen, every counter, and the five tasks the app exists for, walked step by step.
Built 2026-10-07 after his round-1 phone test (`tasks.md` § Stage 2, R1-7), because the
issues he found were all of one kind: a step nobody had walked from the user's side.

**What is here**

- `inventory.md`: every tappable element (467), generated from the source by
  `scripts/interaction-inventory.py`. Rerun it after UI work; a diff shows what moved.
- This file: the screen map, the counters, and the walkthroughs, written by hand from the
  code and the mock build at phone width (360 px). ⚠️ Not yet walked on the S9: round 2 of his
  phone test is that check.

**Method.** Each walkthrough step is put to the four questions of a *cognitive walkthrough*:

1. **Goal** — will the user know this step is what they need to do?
2. **Visible** — can they see the control for it?
3. **Connect** — will they connect that control to what they want?
4. **Feedback** — after doing it, can they tell it worked?

A step that fails one is a finding. Taps are counted from the app already open on another tab
(opening the menu and choosing a tab is 2 taps on a phone).

## Screens

| Tab | Screens inside it | How you get between them |
|---|---|---|
| Journal | the day's entry, calendar | calendar button |
| Notes | list, editor | tap a note; back |
| Routines | list, routine detail, editor | tap; back |
| Finances | **Overview** (net worth, institutions, review inbox, recent), **Ledger** (transaction list → detail), **Analyze** (dashboard, budgets, recurring, reconcile, balance check, accounts), **Add** (photo, PDF, email, manual, statement import, journal import) | the Overview/Ledger/Analyze/Add bar; review-inbox rows; back |
| Archive | list (search, kind/tag/"Unchecked only" filters, retention panel), document detail, add photo/PDF, purge preview | tap a document; back |
| Assistant | home (queues, memory, permissions, conversations, composer), conversation, suggestions inbox, memory, permissions | rows on the home screen; back |
| Settings | sections: server, features, sources, accounts, assistant, check-in, export/import | scroll |

## Counters: what each promises, and whether the place it opens delivers

A counter is a promise: "there are N things for you, and tapping takes you to them". The
failure this table looks for is a number that does not match what its destination shows.

| Counter | Where | Counts | Tap lands on | Delivers? |
|---|---|---|---|---|
| Nav badge, Finances | menu | pending import batches + finance-claimed assistant suggestions | Finances Overview, review inbox highlighted | ✅ |
| Nav badge, Archive | menu | documents with an unchecked field | Archive with "Unchecked only" on | ✅ after R1-1 (it counted never-read documents, 795 extra on dev) |
| Nav badge, Assistant | menu | assistant suggestions no domain screen claims | Assistant home | ✅ |
| "N suggestions waiting for you" | Assistant home | the same | suggestions inbox | ✅ |
| "N items waiting in X" | Assistant home | the other tabs' badges | that tab, at its queue | ✅ (same fix as the Archive badge) |
| Auto-imported batches | Finances review inbox | pending batches | batch list | ✅ |
| Assistant suggestions | Finances review inbox | finance suggestions | suggestions list | ✅ |
| **Unmatched to reconcile** | Finances review inbox | ❌ **was the clearing balance** (-1.50 CAD over ~380 rows on dev) | Reconcile | ✅ fixed `c804523`: now the row count |
| **Unmatched balance** card | Finances › Analyze › Dashboard | balance; said "Everything reconciles" whenever it netted to zero | Reconcile | ✅ fixed `c804523`: pending is now decided by the count |
| Retention "Review N…" | Archive retention panel | documents past the rule | purge preview listing them | ✅ |
| Routine "done/total" | Routines | today's checks | (not a link) | — |

⚠️ **Not counted anywhere: the reconcile queue on the menu badge.** Finances' badge counts
batches and suggestions, not unmatched rows, so ~380 transactions waiting were invisible from
the menu. Design call, listed below (D3).

## Walkthroughs

### T1 — Reconcile this week's bank lines

| Step | Taps | Fails |
|---|---|---|
| Menu → Finances | 2 | — |
| Find "Unmatched to reconcile" in the review inbox (below the hero and chart on a phone) | scroll | ~~Goal: showed "-1.50 CAD", which reads as nothing to do~~ fixed |
| Tap it → Reconcile | 1 | — |
| Per pair: read both sides, Merge | 1 each | ~~Connect: descriptions truncated at ~150 px~~ fixed R1-4 · ~~Feedback: list blanked to "Loading…" after each merge~~ fixed R1-5 |
| Or: "Merge N high-confidence pairs…" → read the list → Merge N | 2 for all | new, R1-6 |
| Per row without a pair: category (suggested where history agrees) → Resolve | 1 with a suggestion, ~4 without | new, R1-6 |

Before: 3 + 1 per pair + ~4 per row, with a reload between each. On dev's backlog (~380
rows) that was ~1,500 taps. Now 3 + 2 for every confident pair + 2 for every suggested row,
then the remainder one by one.

### T2 — Check what a document says

| Step | Taps | Fails |
|---|---|---|
| Assistant "N items waiting in Archive", or the menu badge | 1–2 | ~~Goal: 799 for a real 4~~ fixed R1-1 |
| Opens Archive with "Unchecked only" on | — | — |
| Tap a document → detail (the document beside "What we know") | 1 | — |
| "All N look right", or correct one field | 1 | — |
| Back to the list for the next one | 1 | D1: no "next unchecked", so 3 taps per document |

### T3 — Capture a receipt

| Step | Taps | Fails |
|---|---|---|
| Menu → Finances → Add | 3 | — |
| Photo (or Archive › + Photo) → camera → shutter | 2–3 | his round-1 report: works on the S9 (G9) |
| Wait for extraction → confirm form (description, postings) → Save | 1–3 | Feedback: confirmation line on save ✅ |
| From Archive, a receipt also files into Finances review | — | ✅ says so: "It reads as a receipt, so it is also waiting in Finances review." |

### T4 — Approve an import batch

| Step | Taps | Fails |
|---|---|---|
| Menu → Finances → review inbox "Auto-imported batches" | 3 | — |
| Tap a batch → review (rows with on/off, Edit) | 1 | — |
| "Commit all N" or Dismiss → back to the list | 1 | D2: one batch at a time, 2 taps each, no bulk path (dev had 119 proposed since 09-30) |
| Fully invented batches (promotional emails read as receipts) | — | ~~reached review~~ fixed R1-2: an email that prints none of the amounts proposes nothing |

### T5 — Ask the assistant something

| Step | Taps | Fails |
|---|---|---|
| Menu → Assistant | 2 | ~~Visible: Ask beside a taller box read as misplaced~~ fixed R1-3 |
| Type, Ask → the conversation opens and waits for the server | 1 | Feedback: a waiting state shows; the answer arrives by sync even if the app closes ✅ |
| A proposal appears in the conversation: Accept / Decline | 1 | ✅; the same proposal also waits in the suggestions inbox |

## Findings

### Fixed (clear bugs; his ruling: fix what the map finds)

| # | What | Commit |
|---|---|---|
| R1-1 | Archive/assistant counted never-read documents as unchecked (799 for 4) | `aea6767` |
| R1-2 | Promo emails with empty text became invented receipts, and one grouped five | `7de7a08` |
| R1-3 | Assistant home 32 px narrower than every tab; Ask misaligned | `9987b62` |
| R1-4/5 | Reconcile cards truncated; list reloaded after every merge | `f4fb517` |
| M-1 | "Unmatched to reconcile" showed the balance, not the count; the dashboard card said "everything reconciles" when rows netted to zero | `c804523` |

### For his ruling (design changes, not applied)

- **D1 — "Next unchecked" in document detail.** After confirming, jump to the next unchecked
  document instead of back to the list: 2 taps per document instead of 3.
- **D2 — Bulk commit in the batch list.** The reconcile pattern (list first, one confirm) for
  batches whose arithmetic checked and whose sender authenticated. Same irreversibility
  reasoning as R1-6: he still approves every batch, just as a list.
- **D3 — Reconcile on the Finances menu badge.** Count unmatched rows into Finances' badge so the
  queue is visible from anywhere. Against it: it would make the badge rarely zero while the
  backlog lasts, which trains people to ignore badges.
- **D4 — The WS driver's scheduled orders** (`recurring_investment_policy-*`, "Recurring order
  upcoming: buy TBD") arrive as transactions. Skip them in the box's driver. ⛔ Touches live's
  feed, so his call.

## How to keep this true

Regenerate `inventory.md` after UI work and read the diff. A new counter belongs in the
counters table with its destination, and a new task flow gets a walkthrough before it ships.
