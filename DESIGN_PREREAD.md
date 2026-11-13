# Pre-read: what reaches you, and how the app's agents are organised

**Status 2026-11-12: Part 1 is ruled, and Part 2 is partly ruled.** The rulings are in
`project.md` § Notifications and § Agent framework. Part 2's decisions 2–5 wait for the
architecture study (`tasks.md` [0076], then [0078]). The text below is the pre-read as written.

For one design session, ruled 2026-11-09: **(1) notification criticality, finance first,
then (2) an agent framework for the app.** Nothing here is decided. It is an inventory of
what exists, a diagnosis, and my proposals, each marked as mine, so the session can start
on decisions instead of discovery. Read Part 1 first: what reaches you, and how loudly, is
the contract every agent's output has to meet, so settling it narrows Part 2.

Out of scope: the dev-side question of per-area "process owner" agents for building
omni-me. That one is owed after the next release, and it is about how the code gets
written, not about what runs inside the app.

---

## Part 1 — Critical versus supplementary

Your words: *"so I don't get inundated with notifications and basically tune everything
out or start missing the important ones hidden in the noise."*

### What reaches you today (verified in the code, 2026-11-09)

| Surface | What puts something there | How it reaches you |
|---|---|---|
| Tab badges | Assistant inbox, domain proposals, auto-import batches **plus** unmatched transactions (one Finances count) | A number on the tab, hidden at zero. Seen only when the app is open |
| Chat list | The daily check-in, when switched on (off by default) | A conversation marked as asked on your behalf |
| Settings, source row | A bank source that needs a one-time code | An inline "Reconnect" callout, **only if you open that screen** |
| Archive | Documents read and filed, the purge queue | Nothing: unchecked readings are deliberately not counted (your ruling, 2026-10-10) |

**There are no phone notifications at all.** Nothing interrupts you today, so the noise
problem hasn't started yet. Go-live is what starts it, and it is cheaper to design the rule
now than to retrofit one after you've learned to ignore the badge.

### What go-live switches on, finance first

| Source | Volume | Today's treatment |
|---|---|---|
| Mail receipts and statements → proposed batches | Every charge email, out of ~60 documents filed a day of all kinds (the estimate behind your 2026-10-10 ruling) | Finances badge |
| Bank pulls → proposed transactions | Every card and account movement | Finances badge |
| Unmatched transactions to reconcile | Every leg without its pair | Same Finances badge, same weight |
| Likely-duplicate flags | Rare | Shown on the proposal only |
| A source failing, or needing a code | Roughly monthly per bank, plus outages | Settings row only: **silent unless you look** |
| Recurring charge detected | Occasional | Shown in budget screens |
| Purge queue (retention) | Steady, once timers are set | Archive screen |
| Check-in summary | Once a day, if on | Chat list |

### The gap the table shows

The two things most worth interrupting you for **are the quietest**. A source that stops,
whether it needs a code, its session expired, or the mailbox moved, means money stops being
recorded with no visible sign. It's the failure that leaves the ledger quietly wrong for
weeks. Meanwhile the loudest surface, the Finances badge, adds routine review (a coffee
receipt) to reconciliation backlog in **one number**. A badge that is never zero teaches
you to stop reading it, which is exactly the tune-out you named.

Things that would count as critical but don't exist yet: an unexpected large charge, a
charge from an unknown merchant, a balance that no longer matches a statement (the
"statements as the source-of-truth health check" item already in `tasks.md`), and money due
with a penalty date (rent, tuition).

### A model to start from (my proposal)

Three tiers, defined by **what happens if you never see it**, not by how important it
feels:

| Tier | Test | Delivery | Budget |
|---|---|---|---|
| **Critical** | Missing it costs money or leaves the record wrong, and waiting makes it worse | Phone notification, immediately, through quiet hours only if you allow it | Capped, e.g. a few a week. Over the cap is a design bug, not a reason to send more |
| **Review** | It needs a decision from you eventually, and nothing breaks by waiting a day | No interruption. One daily digest ("12 receipts, 3 to reconcile") at your hour, plus the badge | Unlimited volume, one message |
| **Ambient** | Useful to know, needs nothing from you | Never notifies. Visible where you'd look (the screen, the check-in, an answer) | — |

Rules that make the tiers hold, borrowed from how alerting is run elsewhere:

- **Every critical item is actionable**, and tapping it lands where the action is. A
  critical item you can't act on is ambient wearing a costume.
- **Tier is decided by the item's kind, in code, with a written rule.** A model doesn't
  pick it per item. An LLM may *suggest* that something looks unusual, but whether that
  interrupts you is a rule you set.
- **Critical has a budget, and it's measured.** If it's exceeded, the tier is wrong and
  the kind gets demoted. This is the frequency cap that every source on fatigue converges on.
- **One channel per tier.** The same item never arrives as a notification *and* a digest
  line *and* a chat message.
- **Separate the badge counts.** Review and reconciliation backlog are different jobs,
  and one number hides which one is growing.

This matches the standard platform shape: iOS interruption levels (passive, active,
time-sensitive, critical) and Android notification channels with importance levels.
Finance apps commonly map their own P0–P3 tiers onto those.

### Decisions for the session (Part 1)

1. **The tier test.** Is "costs money or leaves the record wrong" the right definition of
   critical, or is your line elsewhere?
2. **The finance mapping.** For each row in the go-live table plus the four not-yet-built
   kinds: which tier. My starting proposal: source failing / needs a code → critical;
   statement mismatch → critical; large or unknown charge → critical *once built*, with your
   thresholds; everything else → review or ambient.
3. **Critical's budget,** and whether it may break quiet hours.
4. **The digest's hour,** and whether it rides on the check-in or stands alone.
5. **Whether phone notifications get built at all,** and in which order. They need a
   delivery path that doesn't exist yet (the server has no way to wake the phone), so this is
   real build work. It is *not* go-live blocking unless you rule that the source-failure alert
   must ship with go-live.

---

## Part 2 — An agent framework for the app

Your words: *"our current method seems a lot haphazard."*

### What runs on its own today (verified 2026-11-09)

| Job | Where it runs | Trigger | Model seat | Output lands in | Off switch |
|---|---|---|---|---|---|
| Answer a question | agent | a question event | A (interactive) | chat | Llm feature |
| Daily check-in | agent | calendar hour | B (batch) | chat + assistant inbox | setting, off by default |
| Chat tidying | agent | each pass | none | archives/deletes chats | settings (windows) |
| Search index refresh | agent | new events | local embedder | index | none |
| Mail import | server | per-source poll | C0 triage → C1 extractor | Finances review | `enable_imap`, per-source pause |
| Bank import | server | per-source poll | none (structured) | Finances review | `enable_bank`, per-source pause |
| Old-mail backfill | server | slow tick | reader-seat screen | archive only | its own pause |
| Document enrichment | server | scheduler, capped | C2 reader, C3 transcriber | document fields | off by default |
| Note structuring | server | `POST /notes/{id}/process` | D (structurer) | notes | per call |
| Recurring detection | **device** | app scheduler | none | budget screens | — |
| Journal auto-close | **device** | app scheduler | none | journal | — |
| Retention candidates | **device** | opening the screen | none | purge queue | timers |
| Duplicate flags, reconciliation candidates | inline | on import / on read | none | proposal flags | — |

### Diagnosis (mine)

The pieces are individually sound, and most were built carefully. The proposal gate, signed
events, earned autonomy, one model per job chosen by measurement, and approval living with
the domain are all worth keeping. What's missing is the **shape they share**:

- **Three schedulers on three hosts,** each written for its feature. A job's home was
  decided by where its first caller lived, not by a rule.
- **Every job reinvents the same five things:** trigger, budget (turns, calls, cost), failure
  behaviour, off switch, and how you'd know it ran. Some have all five (mail import), and
  some have almost none (index refresh).
- **No single list of what's running.** The table above had to be assembled by searching
  the code, which is the clearest symptom: you can't reason about agents you can't list.
- **Outputs reach you through whatever surface the feature had,** which is how Part 1's
  gap arose. Nothing asks "what tier is this" because there's no place to ask it.
- **The role table in `docs/src/assistant.md` has drifted** from `credentials.rs`
  (C0–C3 and D exist in code; the doc's table predates some of them).

### Options

**A. A job contract and a registry, built here (my recommendation).** Every autonomous job
declares the same record: *name, host, trigger, model seat, permissions (read / propose /
autonomous), budget, failure policy, off switch, output surface and tier*. A registry lists
them, and one status view shows each one's last run, outcome and spend. Existing jobs move
onto it one at a time; nothing is rewritten for its own sake. This attacks the actual
diagnosis: the problem is the missing common shape, not the LLM plumbing.

**B. Adopt Rig (`rig-core`) as the agent layer.** It is the mature Rust option: one interface
over many providers, agents with tools, embeddings and retrieval. Worth evaluating for the
**provider layer only** (`core/src/llm/`): it could replace hand-written provider code.
It doesn't give scheduling across hosts, the proposal gate, signed events, autonomy grants
or tiers, which is everything the diagnosis names. So adopting it alone would leave the
haphazardness exactly where it is, with a new dependency added.

**C. A general workflow engine** (durable-execution systems of the Temporal kind). Solves
cross-host scheduling and retries thoroughly, but it is a second runtime to deploy and
operate on a 4 GB box, for one user's volume. Overkill unless A proves insufficient.

A beat B and C for me because it's the only option aimed at what's actually wrong. B is
compatible with A and can be judged separately, on whether it removes code.

### Decisions for the session (Part 2)

1. **Is the diagnosis right?** In particular, is "no common shape" the haphazardness you
   feel, or is it something else (too many seats, unclear what the assistant may do,
   results you don't trust)?
2. **The contract's fields,** and which are mandatory.
3. **Where jobs live:** a rule for device vs server vs agent (my proposal: device only for
   what must work offline; server for intake; agent for anything that reasons).
4. **The first job to migrate,** as the proof. My proposal: mail import, since it already has
   most fields and is the one go-live turns on.
5. **Whether to evaluate Rig for the provider layer,** as its own small spike.
6. **Sequencing against go-live:** my proposal is that none of Part 2 blocks go-live, while
   Part 1's source-failure alert might (see decision 5 above).

---

Sources: [Klivvr: push notification strategies for financial apps](https://products-by-klivvr.pages.dev/products/klivvr-consumer-app/articles/push-notification-strategies),
[SuprSend: alert fatigue](https://www.suprsend.com/post/alert-fatigue),
[SuprSend: reducing notification fatigue](https://www.suprsend.com/post/reduce-notification-fatigue-without-losing-engagement),
[Rig (rig-core) overview](https://aitoolsatlas.ai/tools/rig-rust),
[Rust AI agent frameworks, 2026](https://lzwjava.github.io/rust-ai-agent-frameworks-2026-en).
Platform interruption levels and Android channel importance are from my own knowledge, not
re-checked this session.
