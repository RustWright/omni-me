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

## Free time

`planning::fit` is the arithmetic the planning model calls as tools. The model never adds up
minutes itself, and a validator refuses a plan it cannot keep.

**The day runs from when you are actually up.** The window opens at your first activity plus
your morning routine, and closes at your bedtime target less your evening routine. There is
no fixed start: a plan that assumes an early start on a late morning gets ignored whole. The
bedtime is the first one after the morning ends, so a target past midnight works.

**Travel is busy time.** Places are your own data: a name, fragments of event locations that
put an event there, and which place is home. The day starts at home. Before an event at
another known place, the trip there is blocked out. An event whose place is unknown (a one-off
with no location, or somewhere with no travel estimate) leaves you where you were, so it adds
no trip. If the day ends away from home, the trip back sits at the end of the window, on the
assumption that you stay out until then.

**Clashes follow your answer.** Once you say which of two clashing events you are going to,
the other stops counting. Until then both are busy, and travel assumes the earlier one.

**The validator names every problem.** A block is refused if it is empty, starts in the
past, falls outside the window, or overlaps an event, a trip or another block. Each refusal
reads as a sentence, and the model gets them back on the same turn.

**The fit line never blocks.** It compares the minutes a plan asks for (blocks plus the
untimed list's estimates) with the free minutes left after routines that have no set time.
Over is shown, never enforced.

## The day's inputs

`planning::day` gathers what `fit` needs for one date, from four places:

- **Settings** (Settings → Planning): the bedtime target, the fallback hour, and which of your
  routines is the morning one and which the evening one. A routine's length is the sum of its
  items' estimates. A name that matches no routine counts as zero, so a typo plans a longer
  day rather than none.
- **Your first activity**: the first journal entry, routine tick or question you typed that
  day. A scheduled question is the assistant's, so it does not count. Activity before 04:00
  belongs to the night before: a 01:00 journal line is a late night, not an early start.
- **The calendar, your answers and your places**, as read in.
- **Now**: once the day is under way, the window starts at the present, because time already
  gone is not free.

## Plans

A plan is a `plan.set` proposal: blocks written `HH:MM-HH:MM Title` in local time, and an
untimed list written `Title (N min)`. It goes through the same approval gate, inbox and sync as
every other proposal, so nothing new had to be trusted.

**The model's plan is checked before you see it.** `propose` reads the day and refuses any
block that sits on an event, on travel, outside the window or on another block, naming each.
The model fixes the plan and proposes it again in the same run. A refused plan never reaches
you. A plan that only runs over the free time is proposed, with the overage on the card.

**Only an accepted plan writes a record of its own.** Approving one writes
`day_plan_accepted`, with the blocks resolved to instants against the day as it stands then,
so a later change of zone or bedtime cannot move them. Reminders and the view read that record.
Every other state is read off the proposal:

| State | When |
|---|---|
| Proposed | waiting on you |
| Accepted | approved, and no later plan for the day was approved |
| Superseded | a later plan for the same day exists; approving this one is refused |
| Skipped | declined ("Skip today") |
| Lapsed | still waiting when its first block was due, or when its day ended |

A re-plan is a whole new plan for the day, never an edit. Until you accept it, the plan you
accepted earlier stays in force.

A block time before the day's window that fits after midnight is read as after midnight, so a
bedtime past 00:00 can be planned.

**Why planning has no read verbs of its own.** A specialist adds instructions and context; it
never adds a verb. The planning brief carries the free time already worked out for today, while
any of it is left, and for tomorrow. The check inside `propose` does the rest. A `free_time`
verb would have let the model ask about any date, but it would have cost a model turn on every
plan, and plans are almost always for today or tomorrow. Planning stays out of the read
catalogue, so the brief is also where the model learns `plan.set`'s name, and a test holds it
to naming every planning action.

## The card and the view

A plan reaches you as a card in chat, and an accepted plan is shown in the Plan view beside
chat. Both are read from `planning::view`, which does every sum and formats every time, so the
app does no zone arithmetic and the two cannot disagree about a day.

**The card shows the day the plan was checked against.** Calendar events and travel sit
inline and greyed among the blocks, so the fit line can be read against the day. The fit is
worked out against the free time as it stood when the plan was drafted, the same check
`propose` ran. The fit line is the only coloured line: amber when over, green within, and it
never blocks Accept.

**A re-plan collapses the old card to one line** ("Plan for Thursday, 09:42 · replaced by
10:05"), and each block that sits somewhere else than a same-titled block in the plan before
it is marked "moved". "Skip today" declines with the reason `skip today`, which is what the
status reads back as Skipped. A card that cannot be read falls back to the ordinary proposal
card, so a plan is never left undecidable.

**The view is one scroll** (ruling 110): what is left of today, with a line where now is, then
the rest of the week as planned-against-free rows. On a Sunday the rows are the week ahead.
Before 04:00 the view still shows the night before's day, as first activity does. The month's
goals close the scroll.

**The view is read-only, so a tapped row opens chat about it.** A block, event, due task or
day opens the thread that day's newest plan was proposed in, since re-plans stay in their thread
(ruling 112). A day with no plan, and a goal, start a new conversation. The composer opens
holding "About 14:15–15:45 Thesis ch. 2: " and nothing is sent: you finish the question, so a
tap spends no model call.

## Goals

A month's goals are a few lines in your own words (ruling 134), set by the monthly review as a
`goal.set` proposal: the month as `YYYY-MM` and at most five lines. Approving one writes
`month_goals_set`, and a later set for the same month replaces the earlier one whole, as a
re-plan replaces a plan. The view lists them as set, with no progress line. The planning brief
carries this month's goals, so a plan can serve them and the review can see what was set.

## Reminders

omni-me fires its own block reminders (ruling 96): the phone shows one when each block of an
accepted plan begins, with a **Start** button that starts the block's timer (ruling 93).

**The app reconciles rather than remembers.** At every start, after every pull that lands
events, after a decision on the phone and every ten minutes, it works out the reminders the
accepted plans call for (`planning::reminders::upcoming`) and brings the phone's scheduled
notifications in line: it cancels any not wanted and schedules any missing. Remembering what it
had scheduled would not survive a force-stop, which cancels every scheduled reminder; the plugin
restores them only after a reboot. A re-plan therefore moves its reminders on the next
reconcile, with no bookkeeping of its own.

**The Start button names its block in its id.** The notification plugin reports only the id of
the action tapped, so each block gets its own action, `start:<block start in unix seconds>:<token>`.
The token is a secret made once per install and kept in the app's private data. The app's
launcher accepts intents from any app, so without it another app could start a block by sending
a made-up action. The notification id is the start in minutes since the epoch; blocks never
overlap, so two reminders never share one.

**Start works with the app closed.** The plugin delivers a tap only to a listener that already
exists, and a tap that cold-starts the app arrives before any does. So `MainActivity` reads the
action off the launch intent itself, leaves it in a side file and, if the page is loaded, tells
it. The page takes it at mount and on that signal, records the start and opens the Plan view
with "Started …" at the top. A Start whose block is no longer in the plan, as after a re-plan
moved it, is refused with a line saying so: a stale reminder never times a block that is not in
the plan.

**The timer lives on the view.** The block now under way offers Start; a started one shows "40
min working · 52 min since start" with Pause and Stop (Resume and Stop while paused), the view's
one exception to read-only (ruling 93). Working time counts only the running spans, and time
since start keeps the breaks, which is what scheduling calibrates on. A block whose timer is
still open stays on the view past its end. Each tap writes `block_timer_started`, `_paused` or
`_stopped`; a start after a pause resumes, and a pause or stop on a block with no open session
is refused.

**One nudge, 30 minutes past the block's end, if its timer is still running.** It is a reminder
like the others, so the reconcile schedules it while the timer runs and cancels it once paused
or stopped; each timer tap wakes the reconcile. Once its time has passed it is never scheduled
again, so it fires at most once. Its id is the negated block id, and it carries no action.

**Exact alarms.** The app declares `USE_EXACT_ALARM`. Without it Android 12 and later batch the
alarm and a reminder can arrive minutes late, and from Android 14 the alternative,
`SCHEDULE_EXACT_ALARM`, is denied by default. Android 13 and later also ask once for permission
to show notifications; a refusal leaves reminders unshown, and the next start asks again.

**What the timer records.** Each timer event carries the plan's date, the block's start and its
title. The `block_timers` table keeps each block's first start and its timer: open session,
running span, working time and time since start, as a task's timer does.

## The morning ritual

The morning plan is drafted when your day starts: at your first activity, or at the fallback
hour if nothing has happened by then. It is never drafted during quiet hours, and it is skipped
when you have already asked for a plan that day. The agent raises it as a scheduled question
(`morning_plan`). That question skips the router and is answered under the planning brief, on
the scheduled-run seat.

The server sends the one silent push for it, outside quiet hours, once a day, because the
server holds the notification channel. The same server tick closes any plan that has lapsed or
been replaced, so the inbox never holds a card that cannot be accepted. The plan you asked for
in chat is not pushed: you are already looking at it.

## The other rituals

The evening shutdown, the weekly plan and the monthly review (ruling 98) are scheduled prompts
too, raised at the summary hour and answered under the planning brief:

| Ritual | When | What it asks |
|---|---|---|
| Evening shutdown (`evening_shutdown`) | daily, except Sunday | each planned item not said done: done, moved to another day, or dropped |
| Weekly plan (`weekly_plan`) | Sundays | how last week went, then at most 10 task decisions, the rest next week |
| Monthly review (`monthly_review`) | the 1st | how last month's goals went, then this month's as a `goal.set` card |

**What surfaces on its own reaches you through them** (rulings 91, 92). The planning brief ends
with "Waiting on the user", built in code, which only the weekly plan asks about (any other
answer mentions an item only when asked): open `USER` or dated items from the project task lists
that you have not taken over, course deadlines still ahead with no task attached, one "still
current?" line for each task list unchanged past Settings → Planning's threshold (21 days) in
place of its items, and native tasks untouched past the other threshold (28 days), which only the
weekly plan offers to drop. A repeating task is never offered, since it sits untouched between
occurrences by design. Each part lists at most ten. Not yet built: the hub trial's record of
acted against dismissed (92), and closing a course's project at term end (123), which waits on
the LEARN feed.

**The brief carries what each ritual looks back on.** The last seven days' accepted plans, one
line a day with any time a block's timer recorded, and last month's goals beside this month's,
are built in code. The weekly plan and the monthly review read them there rather than searching:
without them the first weekly run spent 145k tokens looking for "a weekly plan note".

**A first run waits for its own day.** A ritual that has never run is due only on its day, so
the first weekly plan comes on a Sunday and the first monthly review on the 1st, not on the
evening it was switched on. After that, a run the agent missed still happens when it is back.

The weekly plan covers its own evening, so Sunday has no shutdown. Nothing moves without your
answer: a moved item becomes a task scheduled for its new day, so the next morning's draft
picks it up, and tomorrow is never planned the evening before.

**None of them pushes.** Each rides the 19:00 summary as one line built in code, such as
"Evening shutdown: 4 planned items to settle" or "Weekly plan: 5 of 7 days planned last week,
3 tasks due this week", and the tap opens chat. The line says the ritual is due today; it does
not wait for the agent's thread, so the summary cannot run ahead of it and miss the day.

**A ritual skipped five times running pauses itself** (ruling 99), the morning plan included.
A run counts as skipped when you neither wrote in its thread nor decided a card in it before the
next one. Saying "Skip today" is a decision, so it counts as taking part. A card the server
closed because it lapsed or was replaced does not. The summary says a ritual has paused once,
and answering in any of its last five threads resumes it: the rule reads the threads, so there
is no switch to keep in step. Each ritual lapses on its own record, so skipping mornings never
pauses evenings.

**Their load is measured, never enforced** (ruling 99, as the alert budget is in 55). On
Sundays the summary adds a line when the week went over an aim: about 30 minutes of rituals,
or more than 10 decisions in the last weekly plan. Minutes are an estimate read off the
threads: from your first reply or decision in a run to your last, plus two minutes for reading,
and never more than an hour for one run, since a reply hours later is a gap rather than an hour
spent. Decisions are the cards the weekly plan proposed. The aim of at most two planning pushes
a day holds by construction for now: the morning plan is the only one, and it pushes at most
once a day.

## Places

Places and travel times are your own estimates until location history supplies real ones. They
are set on the server:

```
curl -X POST "$SERVER/planning/places" -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"name":"Campus","matches":["B2","North Hall","Library"],"home":false}'
curl -X POST "$SERVER/planning/travel" -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' -d '{"a":"Home","b":"Campus","minutes":45}'
curl "$SERVER/planning/places" -H "Authorization: Bearer $TOKEN"
```

A fragment matches an event's location as whole words, without case, so `GYM` never matches
inside "Gymnasts Club".
