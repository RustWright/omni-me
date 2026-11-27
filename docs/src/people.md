# People

People is the reference for everyone you have interacted with and recorded something about
(ruling 101). It is not a CRM: wide in storage, focused in presentation. This page is the data
model the build follows ([0129]); the cards and views are `design/0102/` §§ 2–3.

## Two tiers

**Mentions are pointers.** A mention is one line in a source (a journal entry, a task's text, an
email's sender) that names someone. The index records the line, the name as written and, once
known, the person. It is automatic and never asks to be confirmed (ruling 102).

**Facts are claims you confirmed.** A fact is its own record, neither a note nor a belief: the
person, a kind, one line in your words, the date it was true as of, the line it cites, and your
confirmation (ruling 128). The kinds are relationship, work or school, life event, identifying
detail, and where they live or can be found. Health, an address and money are never proposed;
they are recorded only when you ask in chat. Interactions are mentions, never facts.

## Every answer is evidence, and the latest one wins

A mention is resolved by evidence, never by editing it (ruling 104). An answer is one of:

- **link**: this mention is that person (a new person is a provisional person, then a link);
- **alias**: that person under another spelling, hidden (a typo) or shown ("also Shara");
- **unlinked**: nobody you will name, and never asked about again;
- **cleared**: the answer is undone, and the mention is open again.

Each is an event keyed by the mention, so a later answer replaces the earlier one, as calendar
answers do. Undo is a `cleared` answer. Nothing is deleted to undo it, which is what lets the
person view list "how this record was built" with an Undo on each line. "Not this person" is
separate (`mention_excluded`), because one mention can rule out several people.

A guessed link (ruling 103) is an answer too, written by the matcher and marked as guessed. A
guess never replaces one of your answers; a cleared answer opens the mention to guesses again.

## Merge is a mapping

Two records for one person are merged by recording that one now resolves to the other. Every
mention, fact and alias keeps the id it was written with, and readers follow the mapping. Unmerge
removes the mapping, and every attached record is back where it was, from the first event. The
alternative, rewriting each attached record on merge, makes unmerge a guess about which records
came from which side.

## A new person is provisional

A person made from one mention stays provisional until a second mention links to them or you
confirm them (ruling 118). The People tab marks provisional people NEW. A provisional person no
mention links to is left off every list, so undoing "new person" needs no delete.

## Email is the sender's to forge

Mail text, display names and signatures never make a confident match (ruling 103). Only a sender
address you confirmed, on mail that passed sender authentication, links by itself. Confirming an
address is its own event, written when you link an email mention.

## Finding mentions

A server job (`people::finding`) runs every 15 minutes over yesterday's and today's sources:
journal lines, task titles and notes (ruling 124), and the senders of archived email. Each
source line is read once, tracked by a hash of its text, so an edited line is read again as a
new line and a guess you undid is never made again from the same line. Older sources get no
mention here. They wait for the seed import, since a question about them would arrive after the
evening window has closed (ruling 106).

**Known names are matched locally, new names by role D** (ruling 136). The local pass takes runs
of up to three capitalised words that name someone on file, by full name, one word of it, or an
alias, preferring the longest. A lowercase word never matches, so "will" in prose stays a word.
Role D, the zero-retention seat (107), reads the same lines and lists every person they name.
A name the line does not contain is treated as invented and dropped. When exactly one person on
file has that spelling, the mention gets a guessed link; otherwise it waits for the evening
question. A failed model call records nothing and marks nothing read, so the lines are read
again next run.

**Email: only a person's mail is a mention** (ruling 137). The mailbox is mostly vendors, and
asking about each of them every evening would drown the real questions.

| Sender | Result |
|---|---|
| A mailbox's own address, or an automated one (`noreply`, `notifications`, `alerts`…) | nothing |
| A confirmed address, on mail that passed sender authentication | mention, guessed link (103) |
| A confirmed address, on mail that failed authentication | mention, asked |
| An address silenced by "unlinked" | nothing |
| A display name matching someone on file | mention, asked (a display name is never confident) |
| Any other display name | role D, shown name and address, says whether a person is writing; if so, a mention, asked |
| No display name | nothing |

An email mention carries the sender address. "Unlinked" on any mention from that address
silences it, and undoing the answer (a cleared answer) reopens it. The mention's line is the
subject, which is what the person view shows as "Last".

## Evening questions

A mention with no answer is asked about in the evening (ruling 104; cards in `design/0102/` § 2).
`people::questions` builds the cards from local records with no model, so they are asked with the
assistant down, for the same reason the People tab works without it (ruling 119). They sit behind
a row at the top of the Assistant tab, and the 19:00 summary counts them on one line that opens
that tab (ruling 98). The alternative was hanging the cards off the evening shutdown's thread, but
that thread skips Sundays, pauses after five skips (ruling 99), and exists only once the model has
answered.

**When.** Today's mentions are asked from the summary hour. Yesterday's stay up all day and lapse
at midnight, with the line left unlinked: a source gets the evening it was written and one more day.
Guesses are never asked. "Wrong?" on one rules that person out and opens the mention again, so it
comes back as a question while it is still in the window.

**Which card.** A mention with a candidate gets a picker: everyone on file by that spelling, plus
the owner of an email's sender address, less anyone ruled out. Names with no candidate share one
group card per source. A row keeps its card once answered: the person "new person" made for it, and
an alias only that mention taught, never count as its candidates. Without that, answering would
move the row to another card and renumber the set.

| Tap | Evidence written |
|---|---|
| A candidate, or someone found in "Someone I know…" | link; on email, the address confirmed too |
| New person / Someone new | a provisional person with a fixed id per mention, then a link |
| Don't know | unlinked; on email, the sender's other mail is silenced as well (ruling 137) |
| The typo question after picking another spelling | alias, hidden for a typo, shown otherwise |
| Undo | cleared; the confirmed address goes too, unless another answer still names that person |

Undo, then answering "new person" again, reuses the same person, so nothing is left behind.

## Mention ids

A mention's id is the source, a stable hash of the line, and the name. Re-reading a line adds
nothing, and two people of one name in one entry stay two mentions. The hash is FNV-1a, pinned by
a test, because a hash that changed between builds would orphan every stored answer.
