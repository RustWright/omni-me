# Planning

Planning reads your calendar, finds the free time in your day, and proposes a plan for it.
The product rules behind it are rulings 96–117, 125–126 and 133 in `RULINGS.md`. This page covers
how the pieces are built and why.

## Calendar

The calendar is only ever read. Events flow into omni-me, anything new is created in
omni-me, and nothing is written back. So no calendar action is irreversible, and approving a
plan is safe to hand to autonomy later.

**A source returns a snapshot, and the core works out what changed.** A connector implements
one trait, `CalendarSource`: given a window of instants, it returns every occurrence that
overlaps it, with recurring series already expanded. Every 15 minutes, and before every plan,
the core asks for the window from yesterday to eight weeks ahead. It compares the result with
what it has recorded and appends one `calendar_changed` event holding only the difference. An
unchanged calendar writes nothing.

Provider sync tokens were the alternative. They save bandwidth, but they bring token state, a
full-resync path when a token expires, and a sync that cannot be limited to a date window.
Fetching eight weeks for one person is a page or two per calendar, so the snapshot is cheaper to
get right. Recorded events before the window are kept as history: the window moving forward is
not a deletion.

**A failed read changes nothing.** A source reports failure as an error, never as an empty
list. An empty list would mean every event inside the window had been deleted.

**The public engine ships no connector.** It holds the provider-neutral event, the trait and
everything planning does with the result. The Google Calendar connector belongs to the
deployment's own composition root, as bank drivers do (ruling 133). To use another calendar,
implement the trait in your own server crate.

### What a day holds

`calendar::day::read_day` reads one local day:

- **All-day events are labels.** They take no time. One that might take the whole day (a
  conference, a trip) is asked about once, and the answer holds for its series. Yearly series
  (birthdays, anniversaries) are never asked about.
- **Every timed event is busy.** The calendar's "show me as available" flag is ignored: in
  practice it is set on flights and tours booked from email, which are real commitments.
- **Overlaps count once** toward busy time. Exact duplicates (same title and time) merge
  without comment. Any other overlap is a clash, and the plan asks which one you are going to.
  That answer holds for that day only.
- **A recurring series with no location is asked about once**: where does it happen? A one-off
  with no location gets no travel buffer and no question.

Answers are events too (`calendar_answered`), keyed so that a later answer replaces an
earlier one.
