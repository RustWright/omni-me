# People

People is the reference for everyone he has interacted with and recorded something about
(ruling 101). It is not a CRM: wide in storage, focused in presentation. This page is the data
model the build follows ([0129]); the cards and views are `design/0102/` §§ 2–3.

## Two tiers

**Mentions are pointers.** A mention is one line in a source (a journal entry, a task's text, an
email's sender) that names someone. The index records the line, the name as written and, once
known, the person. It is automatic and never asks to be confirmed (ruling 102).

**Facts are claims he confirmed.** A fact is its own record, neither a note nor a belief: the
person, a kind, one line in his words, the date it was true as of, the line it cites, and his
confirmation (ruling 128). The kinds are relationship, work or school, life event, identifying
detail, and where they live or can be found. Health, an address and money are never proposed;
they are recorded only when he asks in chat. Interactions are mentions, never facts.

## Every answer is evidence, and the latest one wins

A mention is resolved by evidence, never by editing it (ruling 104). An answer is one of:

- **link**: this mention is that person;
- **new person**: a provisional person, made from this mention;
- **cannot link**: not that person, so the matcher never offers them for it again;
- **alias**: that person under another spelling, hidden (a typo) or shown ("also Shara");
- **unlinked**: nobody he will name, and never asked about again;
- **cleared**: the answer is undone, and the mention is open again.

Each is an event keyed by the mention, so a later answer replaces the earlier one, as calendar
answers do. Undo is a `cleared` answer. Nothing is ever deleted to undo it, which is what lets the
person view list "how this record was built" with an Undo on each line.

A guessed link (ruling 103) is an answer too, written by the matcher and marked as guessed. A
guess never replaces one of his answers.

## Merge is a mapping

Two records for one person are merged by recording that one now resolves to the other. Every
mention, fact and alias keeps the id it was written with, and readers follow the mapping. Unmerge
removes the mapping, and every attached record is back where it was, from the first event. The
alternative, rewriting each attached record on merge, makes unmerge a guess about which records
came from which side.

## A new person is provisional

A person made from one mention stays provisional until a second mention links to them or he
confirms them (ruling 118). The People tab marks provisional people NEW; nothing else differs.

## Email is the sender's to forge

Mail text, display names and signatures never make a confident match (ruling 103). Only a sender
address he confirmed, on mail that passed sender authentication, links by itself. Confirming an
address is its own event, written when he links an email mention.
