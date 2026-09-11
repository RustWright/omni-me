//! Asking omni-me a question and reading the answer.
//!
//! **Nothing here waits on a model.** Asking appends an event and returns; the
//! answer arrives later, through sync, from the agent that holds the database.
//! So the screen has exactly two states — a question with an answer behind it,
//! and one without — and "without" is not an error until the user decides it is.
//!
//! That shape is why the poll below exists rather than a request that blocks.
//! It also means a question survives the app being closed: the event is durable
//! the moment it is appended, and the answer is waiting on the next launch.

use dioxus::prelude::*;

use crate::bridge;
use crate::components::primitives::{
    Banner, BannerKind, Button, ButtonSize, ButtonVariant, PageHeader,
};
use crate::types::{
    ActionRecord, AssistantMessage, AssistantProposal, AssistantThread, AssistantThreadView, Belief,
};

/// How often to nudge sync while a question is outstanding.
///
/// ⚠️ **Only while outstanding.** The agent polls every 3s because it is on
/// mains power beside the server; a phone doing that continuously would be
/// spending radio for nothing. Between questions this screen is silent and the
/// ordinary background pull carries answers in.
const POLL_MS: i32 = 3000;

/// When to stop reassuring and start explaining.
///
/// A cold answer measured 6.6s end to end, so ~14× that is not a slow answer —
/// it means the agent is not running, or the phone cannot reach the server.
/// Saying so beats a spinner that never resolves.
const OFFLINE_HINT_MS: i64 = 90_000;

#[derive(Clone, PartialEq)]
enum View {
    /// The list of conversations.
    Threads,
    /// One conversation, by id.
    Thread(String),
    /// Everything the assistant has offered to do, across every conversation.
    Inbox,
    /// What it believes about the user, and the evidence behind each one.
    Memory,
    /// What it is allowed to do without asking.
    Permissions,
}

#[component]
pub fn AssistantPage() -> Element {
    let mut view = use_signal(|| View::Threads);

    // Hardware/gesture-back (#372): an open thread is one level deep.
    crate::use_page_back(
        move || match *view.read() {
            View::Threads => 0,
            View::Thread(_) | View::Inbox | View::Memory | View::Permissions => 1,
        },
        move || view.set(View::Threads),
    );

    let current = view.read().clone();
    match current {
        View::Threads => rsx! {
            ThreadList {
                on_open: move |id: String| view.set(View::Thread(id)),
                on_open_inbox: move |_| view.set(View::Inbox),
                on_open_memory: move |_| view.set(View::Memory),
                on_open_permissions: move |_| view.set(View::Permissions),
            }
        },
        View::Thread(id) => rsx! {
            ThreadDetail {
                thread_id: id,
                on_back: move |_| view.set(View::Threads),
            }
        },
        View::Inbox => rsx! {
            ProposalInbox { on_back: move |_| view.set(View::Threads) }
        },
        View::Memory => rsx! {
            MemoryView { on_back: move |_| view.set(View::Threads) }
        },
        View::Permissions => rsx! {
            PermissionsView { on_back: move |_| view.set(View::Threads) }
        },
    }
}

/// The conversation list, plus the composer that starts a new one.
#[component]
fn ThreadList(
    on_open: EventHandler<String>,
    on_open_inbox: EventHandler<()>,
    on_open_memory: EventHandler<()>,
    on_open_permissions: EventHandler<()>,
) -> Element {
    let mut threads = use_signal(Vec::<AssistantThread>::new);
    let mut waiting = use_signal(Vec::<AssistantProposal>::new);
    let mut error_msg = use_signal(|| None::<String>);
    let mut loaded = use_signal(|| false);

    // ⚠️ Refetched on entry and on every inbound sync, never fetched once at
    // boot. A thread answered on another device — or a whole backfill — arrives
    // in a later pull, and a fetch-once provider would never show it. The same
    // goes doubly for proposals: they are authored by the agent, on another
    // machine, and reach this device only through a pull.
    let sync_epoch = crate::sync_refresh::use_sync_epoch();
    use_effect(move || {
        let _ = sync_epoch.read(); // subscribe: re-run on inbound sync
        spawn(async move {
            match bridge::invoke_list_assistant_threads().await {
                Ok(list) => {
                    threads.set(list);
                    error_msg.set(None);
                }
                Err(e) => error_msg.set(Some(e)),
            }
            // A failure here does not blank the screen: the conversation list is
            // still usable without the inbox count.
            if let Ok(list) = bridge::invoke_list_assistant_proposals().await {
                waiting.set(list);
            }
            loaded.set(true);
        });
    });

    let waiting_count = waiting.read().len();

    rsx! {
        div { class: "flex flex-col h-full",
            PageHeader { title: "Assistant".to_string() }

            if let Some(e) = error_msg.read().clone() {
                Banner { kind: BannerKind::Error, "{e}" }
            }

            if waiting_count > 0 {
                div {
                    class: "mx-4 mt-3 px-3 py-2 rounded-md bg-obsidian-accent/10 border border-obsidian-accent/30 cursor-pointer hover:bg-obsidian-accent/15 flex items-center justify-between gap-2",
                    onclick: move |_| on_open_inbox.call(()),
                    div { class: "text-obsidian-text text-sm",
                        if waiting_count == 1 {
                            "1 suggestion waiting for you"
                        } else {
                            "{waiting_count} suggestions waiting for you"
                        }
                    }
                    span { class: "text-obsidian-accent text-xs font-medium", "Review" }
                }
            }

            div { class: "px-4 pt-2",
                button {
                    class: "text-obsidian-text-muted text-xs underline cursor-pointer bg-transparent border-0 p-0",
                    onclick: move |_| on_open_memory.call(()),
                    "What it believes about you"
                }
                span { class: "text-obsidian-text-muted text-xs", " · " }
                button {
                    class: "text-obsidian-text-muted text-xs underline cursor-pointer bg-transparent border-0 p-0",
                    onclick: move |_| on_open_permissions.call(()),
                    "What it may do without asking"
                }
            }

            div { class: "flex-1 overflow-y-auto px-4",
                if *loaded.read() && threads.read().is_empty() {
                    div { class: "text-obsidian-text-muted text-sm py-8 text-center",
                        p { "Ask a question about anything you have written." }
                        p { class: "mt-2 text-xs",
                            "Answers come from the assistant running on your server, so they can take a moment — and they arrive even if you close the app."
                        }
                    }
                }
                for thread in threads.read().iter().cloned() {
                    ThreadRow {
                        key: "{thread.thread_id}",
                        thread: thread.clone(),
                        on_open: move |_| on_open.call(thread.thread_id.clone()),
                    }
                }
            }

            Composer {
                placeholder: "Ask a question…".to_string(),
                on_send: move |text: String| {
                    spawn(async move {
                        match bridge::invoke_ask_assistant(&text, None).await {
                            Ok(asked) => on_open.call(asked.thread_id),
                            Err(e) => error_msg.set(Some(e)),
                        }
                    });
                },
            }
        }
    }
}

#[component]
fn ThreadRow(thread: AssistantThread, on_open: EventHandler<()>) -> Element {
    let title = thread
        .title
        .clone()
        .unwrap_or_else(|| "Untitled conversation".to_string());
    rsx! {
        div {
            class: "px-3 py-3 border-b border-obsidian-border/10 cursor-pointer hover:bg-obsidian-border/5",
            onclick: move |_| on_open.call(()),
            div { class: "text-obsidian-text text-sm font-medium truncate", "{title}" }
            div { class: "text-obsidian-text-muted text-xs mt-1",
                "{thread.message_count} message"
                if thread.message_count != 1 { "s" }
            }
        }
    }
}

/// Everything the assistant has offered to do, across every conversation.
///
/// A screen of its own rather than only cards inside threads, because a proposal
/// made on a conversation the user has scrolled past is one they would never
/// find — and the assistant's whole permission model rests on them seeing what it
/// wants to do.
#[component]
fn ProposalInbox(on_back: EventHandler<()>) -> Element {
    let mut proposals = use_signal(Vec::<AssistantProposal>::new);
    let mut error_msg = use_signal(|| None::<String>);
    let mut loaded = use_signal(|| false);

    let sync_epoch = crate::sync_refresh::use_sync_epoch();
    use_effect(move || {
        let _ = sync_epoch.read();
        spawn(async move {
            match bridge::invoke_list_assistant_proposals().await {
                Ok(list) => {
                    proposals.set(list);
                    error_msg.set(None);
                }
                Err(e) => error_msg.set(Some(e)),
            }
            loaded.set(true);
        });
    });

    let reload = move || {
        spawn(async move {
            if let Ok(list) = bridge::invoke_list_assistant_proposals().await {
                proposals.set(list);
            }
        });
    };

    rsx! {
        div { class: "flex flex-col h-full",
            div { class: "flex items-center gap-2 px-3 py-2 border-b border-obsidian-border/10",
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    onclick: move |_| on_back.call(()),
                    "Back"
                }
                span { class: "text-obsidian-text text-sm font-medium", "Suggestions" }
            }

            if let Some(e) = error_msg.read().clone() {
                Banner { kind: BannerKind::Error, "{e}" }
            }

            div { class: "flex-1 overflow-y-auto px-4 py-3 flex flex-col gap-3",
                if *loaded.read() && proposals.read().is_empty() {
                    div { class: "text-obsidian-text-muted text-sm py-8 text-center",
                        p { "Nothing waiting." }
                        p { class: "mt-2 text-xs",
                            "The assistant cannot change anything on its own. When it wants to, it asks here first."
                        }
                    }
                }
                for proposal in proposals.read().iter().cloned() {
                    ProposalCard {
                        key: "{proposal.proposal_id}",
                        proposal,
                        on_decided: move |_| reload(),
                    }
                }
            }
        }
    }
}

/// One proposal, with the two buttons that settle it.
///
/// ⚠️ Both buttons disable while the decision is in flight. Approving twice is
/// refused by `core` — but the refusal arrives as an error the user did not
/// cause, and a double-tap on a phone is the ordinary way to produce one.
#[component]
fn ProposalCard(proposal: AssistantProposal, on_decided: EventHandler<()>) -> Element {
    let busy = use_signal(|| false);
    let error_msg = use_signal(|| None::<String>);

    let summary = proposal.summary();
    let detail = proposal.detail();
    let evidence = proposal.evidence();
    let cites_evidence = proposal.cites_evidence();
    // Resolved to a finished sentence up front. Any value at all means settled,
    // including one a newer build wrote that this one cannot name.
    let decided = proposal
        .decision
        .clone()
        .unwrap_or_else(|| "decided".to_string());
    // A signal so the closure below stays `Copy` and can serve both buttons; a
    // captured `String` moves into the first one.
    let id = use_signal(|| proposal.proposal_id.clone());

    // A free function rather than a closure: `Signal::set` needs `&mut`, so a
    // closure holding these would be `FnMut` and could not serve both buttons.
    // Signals and `EventHandler` are `Copy`, so passing them costs nothing.
    fn submit(
        mut busy: Signal<bool>,
        mut error_msg: Signal<Option<String>>,
        id: String,
        approve: bool,
        on_decided: EventHandler<()>,
    ) {
        busy.set(true);
        spawn(async move {
            match bridge::invoke_decide_assistant_proposal(&id, approve, None).await {
                Ok(_) => {
                    error_msg.set(None);
                    on_decided.call(());
                }
                Err(e) => error_msg.set(Some(e)),
            }
            busy.set(false);
        });
    }

    rsx! {
        div { class: "rounded-md border border-obsidian-border/20 bg-obsidian-sidebar px-3 py-3 flex flex-col gap-2",
            div { class: "text-obsidian-text text-sm font-medium", "{summary}" }
            div { class: "text-obsidian-text-muted text-xs italic", "{proposal.rationale}" }

            if let Some(body) = detail {
                div { class: "text-obsidian-text text-xs whitespace-pre-wrap bg-obsidian-border/10 rounded px-2 py-2 max-h-40 overflow-y-auto",
                    "{body}"
                }
            }

            // ⚠️ Shown **before** the buttons, and said out loud when absent. A
            // completion asserts that something happened; the user reads their
            // own history back as fact afterwards, so the moment to see what the
            // claim rests on is the moment they are deciding. A card that simply
            // omits the row when there is nothing reads identically to one whose
            // evidence is off-screen — which is the case most worth catching.
            if cites_evidence {
                if evidence.is_empty() {
                    div { class: "text-obsidian-text-muted text-xs italic",
                        "Proposed without opening any record."
                    }
                } else {
                    div { class: "flex flex-wrap gap-1 items-center",
                        span { class: "text-obsidian-text-muted text-xs", "From:" }
                        for cite in evidence.iter() {
                            span {
                                key: "{cite.kind}-{cite.id}",
                                class: "text-obsidian-text-muted text-xs px-1.5 py-0.5 rounded bg-obsidian-border/10",
                                "{cite.title.clone().unwrap_or_else(|| cite.kind.clone())}"
                            }
                        }
                    }
                }
            }

            // ⚠️ Only said when it is true. A blanket reassurance on every card
            // would be worth nothing on the day an irreversible action exists,
            // which is the day it matters most.
            if !proposal.reversible {
                div { class: "text-obsidian-text-muted text-xs",
                    "This one cannot be undone."
                }
            }

            if let Some(e) = error_msg.read().clone() {
                div { class: "text-obsidian-error text-xs", "{e}" }
            }

            if !proposal.is_pending() {
                div { class: "text-obsidian-text-muted text-xs", "You {decided} this." }
            } else {
                div { class: "flex gap-2 pt-1",
                    Button {
                        variant: ButtonVariant::Primary,
                        size: ButtonSize::Sm,
                        disabled: *busy.read(),
                        onclick: move |_| submit(busy, error_msg, id.read().clone(), true, on_decided),
                        "Accept"
                    }
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Sm,
                        disabled: *busy.read(),
                        onclick: move |_| submit(busy, error_msg, id.read().clone(), false, on_decided),
                        "Decline"
                    }
                }
            }
        }
    }
}

/// What the assistant believes about the user, and what it drew each from.
///
/// ⚠️ Retired beliefs are one toggle away rather than gone. The published
/// contract's mitigation for a system that accumulates opinions is that the whole
/// set is *auditable*, and a view that hides what was retired supports
/// inspection, not audit — you could never ask "what did it used to think?".
#[component]
fn MemoryView(on_back: EventHandler<()>) -> Element {
    let mut beliefs = use_signal(Vec::<Belief>::new);
    let mut show_retired = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut loaded = use_signal(|| false);

    let sync_epoch = crate::sync_refresh::use_sync_epoch();
    use_effect(move || {
        let _ = sync_epoch.read();
        let retired = *show_retired.read();
        spawn(async move {
            match bridge::invoke_list_beliefs(retired).await {
                Ok(list) => {
                    beliefs.set(list);
                    error_msg.set(None);
                }
                Err(e) => error_msg.set(Some(e)),
            }
            loaded.set(true);
        });
    });

    let snapshot = beliefs.read().clone();
    let retired_on = *show_retired.read();

    rsx! {
        div { class: "flex flex-col h-full",
            div { class: "flex items-center gap-2 px-3 py-2 border-b border-obsidian-border/10",
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    onclick: move |_| on_back.call(()),
                    "Back"
                }
                span { class: "text-obsidian-text text-sm font-medium", "What it believes" }
            }

            if let Some(e) = error_msg.read().clone() {
                Banner { kind: BannerKind::Error, "{e}" }
            }

            div { class: "px-4 pt-3",
                button {
                    class: "text-obsidian-text-muted text-xs underline cursor-pointer bg-transparent border-0 p-0",
                    onclick: move |_| show_retired.toggle(),
                    if retired_on { "Hide retired" } else { "Show retired" }
                }
            }

            div { class: "flex-1 overflow-y-auto px-4 py-3 flex flex-col gap-3",
                if *loaded.read() && snapshot.is_empty() {
                    div { class: "text-obsidian-text-muted text-sm py-8 text-center",
                        p { "It has not concluded anything about you." }
                        p { class: "mt-2 text-xs",
                            "It only draws a conclusion when you ask it to, and you have to accept each one before it is kept."
                        }
                    }
                }
                for belief in snapshot.iter().cloned() {
                    BeliefCard { key: "{belief.belief_id}", belief }
                }
            }
        }
    }
}

#[component]
fn BeliefCard(belief: Belief) -> Element {
    let live = belief.is_live();
    let card = if live {
        "rounded-md border border-obsidian-border/20 bg-obsidian-sidebar px-3 py-3 flex flex-col gap-2"
    } else {
        "rounded-md border border-obsidian-border/10 bg-obsidian-sidebar/50 px-3 py-3 flex flex-col gap-2 opacity-60"
    };

    rsx! {
        div { class: "{card}",
            div { class: "text-obsidian-text text-sm", "{belief.statement}" }

            div { class: "flex flex-wrap items-center gap-2 text-xs text-obsidian-text-muted",
                span { "{belief.confidence_label()}" }
                if let Some(review) = belief.review_after.clone() {
                    span { "· review after {review}" }
                }
                if !live {
                    span { class: "text-obsidian-text-muted", "· retired" }
                }
            }

            // ⚠️ Said out loud when absent. A conclusion with nothing behind it
            // is the one most worth noticing, and a card that simply omits the
            // evidence row reads identically to one that has evidence off-screen.
            if belief.evidence.is_empty() {
                div { class: "text-obsidian-text-muted text-xs italic",
                    "Drawn without opening any record."
                }
            } else {
                div { class: "flex flex-wrap gap-1 items-center",
                    span { class: "text-obsidian-text-muted text-xs", "From:" }
                    for cite in belief.evidence.iter() {
                        span {
                            key: "{cite.kind}-{cite.id}",
                            class: "text-obsidian-text-muted text-xs px-1.5 py-0.5 rounded bg-obsidian-border/10",
                            "{cite.title.clone().unwrap_or_else(|| cite.kind.clone())}"
                        }
                    }
                }
            }

            if let Some(reason) = belief.superseded_reason.clone() {
                div { class: "text-obsidian-text-muted text-xs italic", "Retired: {reason}" }
            }
        }
    }
}

/// What the assistant may do without asking, and the record behind each choice.
///
/// ⚠️ The evidence is shown **beside** the toggle, not on a separate screen. The
/// whole argument for granting anything is the approval record, and a permission
/// switch with no record next to it is a guess wearing a control.
#[component]
fn PermissionsView(on_back: EventHandler<()>) -> Element {
    let mut records = use_signal(Vec::<ActionRecord>::new);
    let mut error_msg = use_signal(|| None::<String>);
    let mut loaded = use_signal(|| false);

    let sync_epoch = crate::sync_refresh::use_sync_epoch();
    use_effect(move || {
        let _ = sync_epoch.read();
        spawn(async move {
            match bridge::invoke_list_action_records().await {
                Ok(list) => {
                    records.set(list);
                    error_msg.set(None);
                }
                Err(e) => error_msg.set(Some(e)),
            }
            loaded.set(true);
        });
    });

    let snapshot = records.read().clone();

    rsx! {
        div { class: "flex flex-col h-full",
            div { class: "flex items-center gap-2 px-3 py-2 border-b border-obsidian-border/10",
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    onclick: move |_| on_back.call(()),
                    "Back"
                }
                span { class: "text-obsidian-text text-sm font-medium", "Permissions" }
            }

            if let Some(e) = error_msg.read().clone() {
                Banner { kind: BannerKind::Error, "{e}" }
            }

            div { class: "px-4 pt-3 text-obsidian-text-muted text-xs",
                "The assistant asks before doing anything. You can let it stop asking \
                 about things it has got right, and take that back at any time."
            }

            div { class: "flex-1 overflow-y-auto px-4 py-3 flex flex-col gap-3",
                if *loaded.read() && snapshot.is_empty() {
                    div { class: "text-obsidian-text-muted text-sm py-8 text-center",
                        "There is nothing it can do yet."
                    }
                }
                for record in snapshot.iter().cloned() {
                    PermissionRow {
                        key: "{record.action}",
                        record,
                        on_changed: move |_| {
                            spawn(async move {
                                if let Ok(list) = bridge::invoke_list_action_records().await {
                                    records.set(list);
                                }
                            });
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn PermissionRow(record: ActionRecord, on_changed: EventHandler<()>) -> Element {
    let busy = use_signal(|| false);
    let error_msg = use_signal(|| None::<String>);
    let action = use_signal(|| record.action.clone());

    fn toggle(
        mut busy: Signal<bool>,
        mut error_msg: Signal<Option<String>>,
        action: String,
        granted: bool,
        on_changed: EventHandler<()>,
    ) {
        busy.set(true);
        spawn(async move {
            match bridge::invoke_set_action_autonomy(&action, granted).await {
                Ok(()) => {
                    error_msg.set(None);
                    on_changed.call(());
                }
                Err(e) => error_msg.set(Some(e)),
            }
            busy.set(false);
        });
    }

    let granted = record.granted;
    let grantable = record.can_be_granted();

    rsx! {
        div { class: "rounded-md border border-obsidian-border/20 bg-obsidian-sidebar px-3 py-3 flex flex-col gap-2",
            div { class: "text-obsidian-text text-sm font-medium", "{record.action}" }
            div { class: "text-obsidian-text-muted text-xs", "{record.evidence_label()}" }

            if !grantable {
                // ⛔ The rule, stated where the control would otherwise be. An
                // absent toggle with no explanation reads as an oversight.
                div { class: "text-obsidian-text-muted text-xs italic",
                    "This one cannot be undone, so it will always ask."
                }
            } else if granted {
                div { class: "flex items-center gap-2 pt-1",
                    span { class: "text-obsidian-accent text-xs font-medium", "Does this without asking" }
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Sm,
                        disabled: *busy.read(),
                        onclick: move |_| toggle(busy, error_msg, action.read().clone(), false, on_changed),
                        "Ask me again"
                    }
                }
            } else {
                div { class: "pt-1",
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Sm,
                        disabled: *busy.read(),
                        onclick: move |_| toggle(busy, error_msg, action.read().clone(), true, on_changed),
                        "Stop asking me about this"
                    }
                }
            }

            if let Some(e) = error_msg.read().clone() {
                div { class: "text-obsidian-error text-xs", "{e}" }
            }
        }
    }
}

/// One conversation: the transcript, the pending state, and the follow-up box.
#[component]
fn ThreadDetail(thread_id: String, on_back: EventHandler<()>) -> Element {
    let mut view = use_signal(|| None::<AssistantThreadView>);
    let mut error_msg = use_signal(|| None::<String>);
    // Milliseconds the current question has been waiting, as this screen counts
    // it. Reset every time the pending question changes.
    let mut waited_ms = use_signal(|| 0i64);

    let mut proposals = use_signal(Vec::<AssistantProposal>::new);
    // A signal rather than a clone: the proposal cards are built inside an
    // `rsx!` loop, whose closures must be `Copy`, and a `String` captured there
    // moves on the first iteration.
    let card_thread_id = use_signal(|| thread_id.clone());

    let id_for_load = thread_id.clone();
    let sync_epoch = crate::sync_refresh::use_sync_epoch();
    use_effect(move || {
        let _ = sync_epoch.read();
        let id = id_for_load.clone();
        spawn(async move {
            match bridge::invoke_read_assistant_thread(&id).await {
                Ok(v) => {
                    view.set(Some(v));
                    error_msg.set(None);
                }
                Err(e) => error_msg.set(Some(e)),
            }
            if let Ok(list) = bridge::invoke_read_thread_proposals(&id).await {
                proposals.set(list);
            }
        });
    });

    // While a question is outstanding, pull on a short interval. `trigger_sync`
    // pushes, pulls *and* projects, so one call covers getting the question out
    // and the answer back — there is no separate command to add.
    let id_for_poll = thread_id.clone();
    use_future(move || {
        let id = id_for_poll.clone();
        async move {
            loop {
                crate::timer::sleep_ms(POLL_MS).await;
                let pending = view.read().as_ref().and_then(|v| v.pending.clone());
                if pending.is_none() {
                    waited_ms.set(0);
                    continue;
                }
                waited_ms.with_mut(|w| *w += POLL_MS as i64);
                // A failed sync is not worth surfacing here: the next tick
                // retries, and the answer is durable on the server regardless.
                let _ = bridge::invoke_trigger_sync().await;
                if let Ok(v) = bridge::invoke_read_assistant_thread(&id).await {
                    let settled = v.pending.is_none();
                    if settled {
                        waited_ms.set(0);
                    }
                    view.set(Some(v));
                    // Only once the answer has landed: an answer and the
                    // proposals it made are appended in one batch, so before
                    // that there is nothing new to fetch.
                    if settled && let Ok(list) = bridge::invoke_read_thread_proposals(&id).await {
                        proposals.set(list);
                    }
                }
            }
        }
    });

    let snapshot = view.read().clone();
    let pending = snapshot.as_ref().and_then(|v| v.pending.clone());
    let messages = snapshot.map(|v| v.messages).unwrap_or_default();
    let offline_hint = pending.is_some() && *waited_ms.read() >= OFFLINE_HINT_MS;
    let id_for_send = thread_id.clone();

    rsx! {
        div { class: "flex flex-col h-full",
            div { class: "flex items-center gap-2 px-3 py-2 border-b border-obsidian-border/10",
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    onclick: move |_| on_back.call(()),
                    "Back"
                }
            }

            if let Some(e) = error_msg.read().clone() {
                Banner { kind: BannerKind::Error, "{e}" }
            }

            div { class: "flex-1 overflow-y-auto px-4 py-3 flex flex-col gap-3",
                for message in messages.iter().cloned() {
                    MessageBubble { key: "{message.message_id}", message: message.clone() }

                    // Proposals sit under the answer that made them, so the
                    // reasoning and the decision are read together. The inbox
                    // screen is the same cards without that context, for
                    // proposals on conversations the user has moved past.
                    for proposal in proposals.read().iter()
                        .filter(|p| p.message_id == message.message_id)
                        .cloned()
                    {
                        ProposalCard {
                            key: "{proposal.proposal_id}",
                            proposal,
                            on_decided: move |_| {
                                let id = card_thread_id.read().clone();
                                spawn(async move {
                                    if let Ok(list) = bridge::invoke_read_thread_proposals(&id).await {
                                        proposals.set(list);
                                    }
                                });
                            },
                        }
                    }
                }

                if pending.is_some() {
                    div { class: "text-obsidian-text-muted text-sm italic",
                        if offline_hint {
                            div { class: "not-italic",
                                Banner {
                                    kind: BannerKind::Warn,
                                    "No answer yet. The assistant may not be running, or this device cannot reach your server. Your question is saved either way — it will be answered when the assistant comes back."
                                }
                            }
                        } else {
                            "Thinking…"
                        }
                    }
                }
            }

            Composer {
                placeholder: "Ask a follow-up…".to_string(),
                on_send: move |text: String| {
                    let id = id_for_send.clone();
                    spawn(async move {
                        match bridge::invoke_ask_assistant(&text, Some(&id)).await {
                            // Re-read straight away so the question appears in the
                            // transcript without waiting for the first poll tick.
                            Ok(_) => {
                                waited_ms.set(0);
                                if let Ok(v) = bridge::invoke_read_assistant_thread(&id).await {
                                    view.set(Some(v));
                                }
                            }
                            Err(e) => error_msg.set(Some(e)),
                        }
                    });
                },
            }
        }
    }
}

#[component]
fn MessageBubble(message: AssistantMessage) -> Element {
    let is_user = message.is_user();
    let bubble = if is_user {
        "self-end max-w-[85%] bg-obsidian-accent/15 text-obsidian-text rounded-lg px-3 py-2 text-sm whitespace-pre-wrap"
    } else {
        "self-start max-w-[85%] bg-obsidian-sidebar text-obsidian-text rounded-lg px-3 py-2 text-sm whitespace-pre-wrap"
    };

    // An answer that never arrived says why, in prose. `stopped` codes are for
    // the log, not for a person deciding whether to ask again.
    let failure = message.failure_note();
    let citations = message.records_read.clone().unwrap_or_default();

    rsx! {
        div { class: "flex flex-col gap-1",
            // ⚠️ Said before the text, not after. The reader needs to know this
            // was not their question *while* reading it — an answer to something
            // you never asked, rendered like one you did, reads as the assistant
            // volunteering opinions.
            if message.is_scheduled() {
                div { class: "self-end text-obsidian-text-muted text-xs",
                    "Asked on your behalf"
                }
            }
            div { class: "{bubble}",
                if let Some(note) = failure.clone() {
                    span { class: "text-obsidian-text-muted italic", "{note}" }
                } else {
                    "{message.text.clone().unwrap_or_default()}"
                }
            }

            if !is_user && !citations.is_empty() {
                div { class: "self-start flex flex-wrap gap-1 pl-1",
                    span { class: "text-obsidian-text-muted text-xs", "From:" }
                    for cite in citations.iter() {
                        // ⚠️ The event carries no title on purpose, so a renamed
                        // record still cites correctly. Until this resolves the
                        // name from the local record tables, the kind is what
                        // there is to show — an id would be worse than useless.
                        span {
                            key: "{cite.kind}-{cite.id}",
                            class: "text-obsidian-text-muted text-xs px-1.5 py-0.5 rounded bg-obsidian-border/10",
                            "{cite.title.clone().unwrap_or_else(|| cite.kind.clone())}"
                        }
                    }
                }
            }
        }
    }
}

/// The text box and its send button.
///
/// Enter sends and Shift+Enter breaks the line, which is the convention every
/// chat surface shares — but the button stays visible rather than relying on it,
/// because on a phone there is no Enter key doing that job.
#[component]
fn Composer(placeholder: String, on_send: EventHandler<String>) -> Element {
    let mut draft = use_signal(String::new);

    let mut send = move || {
        let text = draft.read().trim().to_string();
        if text.is_empty() {
            return;
        }
        draft.set(String::new());
        on_send.call(text);
    };

    rsx! {
        div { class: "flex items-end gap-2 px-3 py-2 border-t border-obsidian-border/10",
            textarea {
                class: "flex-1 px-3 py-2 bg-obsidian-sidebar border border-obsidian-border/10 rounded-md text-obsidian-text text-sm outline-none focus:border-obsidian-accent resize-none",
                rows: 2,
                placeholder: "{placeholder}",
                value: "{draft}",
                oninput: move |e| draft.set(e.value()),
                onkeydown: move |e| {
                    if e.key() == Key::Enter && !e.modifiers().shift() {
                        e.prevent_default();
                        send();
                    }
                },
            }
            Button {
                variant: ButtonVariant::Primary,
                size: ButtonSize::Sm,
                onclick: move |_| send(),
                "Ask"
            }
        }
    }
}
