//! The evening people questions (rulings 104, 118; `design/0102/` § 2). What is asked is worked
//! out in core's `people::questions`; this lays it out. One tap answers, with Undo and no confirm,
//! because every answer is an evidence event that can be cleared.

use dioxus::prelude::*;

use crate::bridge;
use crate::components::primitives::{Banner, BannerKind, Button, ButtonSize, ButtonVariant};
use crate::sync_refresh::{bump_sync_epoch, use_sync_epoch};
use crate::types::{PeopleQuestions, PeopleReply, PersonChoice, QuestionCard, QuestionRow};

/// A row waiting on the typo question, after he picked someone by another spelling.
#[derive(Clone, PartialEq)]
struct Spelling {
    mention_id: String,
    person_id: String,
    person: String,
}

const OPT: &str = "w-full text-left rounded-md border border-obsidian-border/15 px-3 py-2 flex flex-col gap-0.5 hover:bg-obsidian-border/5 disabled:opacity-50";
const MINI: &str = "text-xs whitespace-nowrap rounded-full border border-obsidian-border/15 px-2.5 py-1.5 text-obsidian-text hover:bg-obsidian-border/5 disabled:opacity-50";

fn number(n: usize) -> String {
    const WORDS: [&str; 11] = [
        "No", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten",
    ];
    WORDS
        .get(n)
        .map_or_else(|| n.to_string(), |w| (*w).to_string())
}

/// "Three people questions from today."
fn opening(q: &PeopleQuestions) -> String {
    let n = q.cards.len();
    let old = q.cards.iter().filter(|c| c.lapses_tonight).count();
    let when = match (old, n - old) {
        (_, 0) => "yesterday",
        (0, _) => "today",
        _ => "today and yesterday",
    };
    let noun = if n == 1 { "question" } else { "questions" };
    format!("{} people {noun} from {when}.", number(n))
}

/// The line, with the name marked where it first appears.
#[component]
fn Quote(row: QuestionRow) -> Element {
    let QuestionRow {
        line,
        name,
        sender,
        more,
        ..
    } = row;
    let at = line.to_lowercase().find(&name.trim().to_lowercase());
    let (before, mark, after) = match at {
        Some(i) if line.is_char_boundary(i) && line.is_char_boundary(i + name.trim().len()) => {
            let end = i + name.trim().len();
            (
                line[..i].to_string(),
                line[i..end].to_string(),
                line[end..].to_string(),
            )
        }
        _ => (line.clone(), String::new(), String::new()),
    };
    rsx! {
        div { class: "text-sm text-obsidian-text border-l-2 border-obsidian-border/25 pl-2.5 py-0.5 break-words",
            if let Some(s) = sender {
                div { class: "text-obsidian-text-muted", "{s}" }
            }
            "{before}"
            if !mark.is_empty() {
                mark { class: "bg-obsidian-accent/25 text-obsidian-text rounded px-0.5", "{mark}" }
            }
            "{after}"
            if more > 0 {
                div { class: "text-xs text-obsidian-text-muted mt-0.5",
                    if more == 1 { "and 1 more email from this sender" } else { "and {more} more emails from this sender" }
                }
            }
        }
    }
}

/// An answered row: one line and Undo.
#[component]
fn Answered(row: QuestionRow, on_reply: EventHandler<PeopleReply>) -> Element {
    let answer = row.answered.clone().unwrap_or_default();
    // "Robert Chen → Robert Chen, address confirmed" says the name twice.
    // Only the exact name: "Sarah → Sarah Okafor" keeps its arrow.
    let said = row.name.trim().to_lowercase();
    let lower = answer.to_lowercase();
    let text = if lower == said || lower.starts_with(&format!("{said},")) {
        answer
    } else {
        format!("{} → {answer}", row.name)
    };
    let id = row.mention_id.clone();
    rsx! {
        div { class: "flex items-center justify-between gap-3 rounded-md border border-obsidian-border/10 px-3 py-2 text-xs text-obsidian-text-muted",
            span { class: "break-words", "{text}" }
            button {
                class: "shrink-0 text-obsidian-accent",
                onclick: move |_| on_reply.call(PeopleReply::Undo { mention_id: id.clone() }),
                "Undo"
            }
        }
    }
}

#[component]
fn ChoiceButton(choice: PersonChoice, busy: bool, on_pick: EventHandler<PersonChoice>) -> Element {
    let c = choice.clone();
    rsx! {
        button {
            class: OPT,
            disabled: busy,
            onclick: move |_| on_pick.call(c.clone()),
            span { class: "text-sm font-semibold text-obsidian-text", "{choice.name}" }
            if !choice.context.is_empty() {
                span { class: "text-xs text-obsidian-text-muted", "{choice.context}" }
            }
        }
    }
}

/// The typo question, in the row: a hidden alias or a shown "also" name (ruling 118).
#[component]
fn SpellingQuestion(
    spelling: Spelling,
    row: QuestionRow,
    on_reply: EventHandler<PeopleReply>,
) -> Element {
    let name = row.name.clone();
    let asked = spelling.clone();
    let answer = move |typo: bool| {
        let s = asked.clone();
        move |_| {
            on_reply.call(PeopleReply::Spelling {
                mention_id: s.mention_id.clone(),
                person_id: s.person_id.clone(),
                typo,
            })
        }
    };
    rsx! {
        div { class: "flex flex-col gap-1.5",
            Quote { row: row.clone() }
            p { class: "text-sm text-obsidian-text",
                "Linked to {spelling.person}. Was \"{name}\" a typo?"
            }
            button { class: OPT, onclick: answer(true),
                span { class: "text-sm font-semibold text-obsidian-text", "Yes, a typo" }
                span { class: "text-xs text-obsidian-text-muted", "Kept hidden, only to match it again" }
            }
            button { class: OPT, onclick: answer(false),
                span { class: "text-sm font-semibold text-obsidian-text", "No, another name" }
                span { class: "text-xs text-obsidian-text-muted", "Shown as \"also {name}\"" }
            }
        }
    }
}

/// "Someone I know…": a search of everyone in the row, prefilled with the name.
#[component]
fn SomeoneIKnow(
    row: QuestionRow,
    busy: bool,
    on_pick: EventHandler<PersonChoice>,
    on_close: EventHandler<()>,
) -> Element {
    let mut query = use_signal(|| row.name.clone());
    let found = use_resource(move || {
        let q = query.read().clone();
        async move { bridge::invoke_people_closest(q).await }
    });
    let list = match found.read().clone() {
        Some(Ok(list)) => list,
        _ => Vec::new(),
    };
    let failed = matches!(*found.read(), Some(Err(_)));
    rsx! {
        div { class: "flex flex-col gap-1.5",
            Quote { row: row.clone() }
            input {
                r#type: "text",
                class: "text-sm bg-transparent rounded-md border border-obsidian-accent/60 px-2.5 py-1.5 text-obsidian-text focus:outline-none",
                value: "{query}",
                autocomplete: "off",
                aria_label: "Search your people",
                oninput: move |e| query.set(e.value()),
            }
            p { class: "text-[11px] text-obsidian-text-muted", "All your people, closest spelling first" }
            if failed {
                p { class: "text-xs text-error", "Couldn't search your people." }
            }
            for choice in list {
                ChoiceButton { key: "{choice.person_id}", choice: choice.clone(), busy, on_pick }
            }
            button {
                class: "self-start text-xs text-obsidian-accent py-1",
                onclick: move |_| on_close.call(()),
                "Back to New person / Don't know"
            }
        }
    }
}

#[component]
fn Kicker(heading: String, place: String) -> Element {
    rsx! {
        div { class: "flex items-baseline justify-between gap-3",
            span { class: "text-sm font-semibold text-obsidian-text", "{heading}" }
            span { class: "text-xs text-obsidian-text-muted shrink-0", "{place}" }
        }
    }
}

#[component]
fn Source(card: QuestionCard) -> Element {
    let tone = if card.email {
        "text-warn"
    } else {
        "text-obsidian-text-muted"
    };
    rsx! {
        div { class: "text-[11px] {tone}", "{card.source}" }
    }
}

const CARD: &str = "bg-obsidian-surface border border-obsidian-border/10 rounded-card shadow-card px-3 py-3 flex flex-col gap-2";

#[component]
fn Picker(
    card: QuestionCard,
    place: String,
    busy: bool,
    on_reply: EventHandler<PeopleReply>,
) -> Element {
    let Some(row) = card.rows.first().cloned() else {
        return rsx! {};
    };
    let (new_id, dk_id) = (row.mention_id.clone(), row.mention_id.clone());
    let footer = if card.confirms_address {
        "Linking also confirms this address, so later mail from it links by itself."
    } else if card.lapses_tonight {
        "Unanswered, this lapses tonight and the line stays unlinked."
    } else {
        "Unanswered, this lapses tomorrow night and the line stays unlinked."
    };
    let pick_id = row.mention_id.clone();
    rsx! {
        div { class: CARD,
            Kicker { heading: card.heading.clone(), place }
            Source { card: card.clone() }
            Quote { row: row.clone() }
            for choice in row.candidates.iter().cloned() {
                {
                    let id = pick_id.clone();
                    rsx! {
                        ChoiceButton {
                            key: "{choice.person_id}",
                            choice,
                            busy,
                            on_pick: move |c: PersonChoice| on_reply.call(PeopleReply::Person {
                                mention_id: id.clone(),
                                person_id: c.person_id,
                            }),
                        }
                    }
                }
            }
            div { class: "grid grid-cols-2 gap-1.5",
                button {
                    class: "{OPT} text-sm font-medium text-obsidian-text",
                    disabled: busy,
                    onclick: move |_| on_reply.call(PeopleReply::NewPerson { mention_id: new_id.clone() }),
                    "Someone new"
                }
                button {
                    class: "{OPT} text-sm font-medium text-obsidian-text",
                    disabled: busy,
                    onclick: move |_| on_reply.call(PeopleReply::DontKnow { mention_id: dk_id.clone() }),
                    "Don't know"
                }
            }
            p { class: "text-xs text-obsidian-text-muted", "{footer}" }
        }
    }
}

#[component]
fn GroupCard(
    card: QuestionCard,
    place: String,
    busy: bool,
    spelling: Option<Spelling>,
    on_reply: EventHandler<PeopleReply>,
    on_pick: EventHandler<(QuestionRow, PersonChoice)>,
) -> Element {
    let mut searching = use_signal(|| None::<String>);
    let footer = if card.lapses_tonight {
        "Unanswered, these lapse tonight and the lines stay unlinked."
    } else {
        "Unanswered, these lapse tomorrow night and the lines stay unlinked."
    };
    rsx! {
        div { class: CARD,
            Kicker { heading: card.heading.clone(), place }
            if card.email {
                Source { card: card.clone() }
            }
            for (i, row) in card.rows.iter().cloned().enumerate() {
                div {
                    key: "{row.mention_id}",
                    class: if i == 0 { "flex flex-col gap-1.5 py-1" } else { "flex flex-col gap-1.5 pt-2.5 pb-1 border-t border-obsidian-border/10" },
                    {
                        let asking = spelling.as_ref().filter(|s| s.mention_id == row.mention_id).cloned();
                        let open = searching.read().as_deref() == Some(row.mention_id.as_str());
                        let (new_id, dk_id, find_id) = (
                            row.mention_id.clone(),
                            row.mention_id.clone(),
                            row.mention_id.clone(),
                        );
                        if let Some(s) = asking {
                            rsx! { SpellingQuestion { spelling: s, row: row.clone(), on_reply } }
                        } else if row.answered.is_some() {
                            rsx! { Answered { row: row.clone(), on_reply } }
                        } else if open {
                            let r = row.clone();
                            rsx! {
                                SomeoneIKnow {
                                    row: row.clone(),
                                    busy,
                                    on_pick: move |c: PersonChoice| {
                                        searching.set(None);
                                        on_pick.call((r.clone(), c));
                                    },
                                    on_close: move |_| searching.set(None),
                                }
                            }
                        } else {
                            rsx! {
                                Quote { row: row.clone() }
                                div { class: "flex flex-wrap gap-1.5",
                                    button {
                                        class: MINI,
                                        disabled: busy,
                                        onclick: move |_| on_reply.call(PeopleReply::NewPerson { mention_id: new_id.clone() }),
                                        "New person"
                                    }
                                    button {
                                        class: MINI,
                                        disabled: busy,
                                        onclick: move |_| on_reply.call(PeopleReply::DontKnow { mention_id: dk_id.clone() }),
                                        "Don't know"
                                    }
                                    button {
                                        class: MINI,
                                        disabled: busy,
                                        onclick: move |_| searching.set(Some(find_id.clone())),
                                        "Someone I know…"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            p { class: "text-xs text-obsidian-text-muted", "{footer}" }
        }
    }
}

/// The questions, one card at a time: answered ones shrink to a line with Undo above the card
/// being asked, and the rest wait behind it.
#[component]
pub fn PeopleQuestionsView(on_back: EventHandler<()>) -> Element {
    let sync_epoch = use_sync_epoch();
    let mut busy = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut spelling = use_signal(|| None::<Spelling>);
    let questions = use_resource(move || {
        let _ = sync_epoch.read();
        async move { bridge::invoke_people_questions().await }
    });

    let mut send = move |reply: PeopleReply, then: Option<Spelling>| {
        busy.set(true);
        spawn(async move {
            let undoing = matches!(reply, PeopleReply::Undo { .. });
            match bridge::invoke_people_reply(reply).await {
                Ok(outcome) => {
                    error_msg.set(None);
                    spelling.set(then.filter(|_| outcome.ask_spelling && !undoing));
                    bump_sync_epoch(sync_epoch);
                }
                Err(e) => error_msg.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let on_reply = move |reply: PeopleReply| {
        // Any other answer settles the typo question it was waiting on.
        send(reply, None);
    };
    let on_pick = move |(row, c): (QuestionRow, PersonChoice)| {
        let then = Spelling {
            mention_id: row.mention_id.clone(),
            person_id: c.person_id.clone(),
            person: c.name.clone(),
        };
        send(
            PeopleReply::Person {
                mention_id: row.mention_id,
                person_id: c.person_id,
            },
            Some(then),
        );
    };

    let body = match questions.read().clone() {
        None => {
            rsx! { p { class: "text-sm text-obsidian-text-muted", "Reading today's questions…" } }
        }
        Some(Err(e)) => {
            rsx! { Banner { kind: BannerKind::Error, "Couldn't read the questions: {e}" } }
        }
        Some(Ok(q)) if q.cards.is_empty() => rsx! {
            p { class: "text-sm text-obsidian-text-muted", "No people questions today." }
        },
        Some(Ok(q)) => {
            let n = q.cards.len();
            let asking = spelling.read().clone();
            // The card with the typo question open stays up even though its row is answered.
            let current = q.cards.iter().position(|c| {
                !c.done()
                    || asking
                        .as_ref()
                        .is_some_and(|s| c.rows.iter().any(|r| r.mention_id == s.mention_id))
            });
            let busy_now = *busy.read();
            rsx! {
                div { class: "self-start max-w-[85%] bg-obsidian-sidebar text-obsidian-text rounded-lg px-3 py-2 text-sm",
                    "{opening(&q)}"
                }
                for (i, card) in q.cards.iter().cloned().enumerate() {
                    if Some(i) == current {
                        if card.picker {
                            Picker {
                                key: "{card.rows[0].mention_id}",
                                card: card.clone(),
                                place: format!("{} of {n}", i + 1),
                                busy: busy_now,
                                on_reply,
                            }
                        } else {
                            GroupCard {
                                key: "{card.rows[0].mention_id}",
                                card: card.clone(),
                                place: format!("{} of {n}", i + 1),
                                busy: busy_now,
                                spelling: asking.clone(),
                                on_reply,
                                on_pick,
                            }
                        }
                    } else if card.done() {
                        for row in card.rows.iter().cloned() {
                            Answered { key: "{row.mention_id}", row, on_reply }
                        }
                    }
                }
                if current.is_none() {
                    p { class: "text-xs text-obsidian-text-muted", "All answered. Each one can be undone above." }
                }
            }
        }
    };

    rsx! {
        div { class: "flex flex-col min-h-full",
            div { class: "sticky -top-4 md:-top-6 z-10 bg-obsidian-bg flex items-center gap-2 px-3 py-2 border-b border-obsidian-border/10",
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    onclick: move |_| on_back.call(()),
                    "Back"
                }
                span { class: "text-sm font-medium text-obsidian-text", "People questions" }
            }
            if let Some(e) = error_msg.read().clone() {
                Banner { kind: BannerKind::Error, "{e}" }
            }
            div { class: "py-3 flex flex-col gap-3", {body} }
        }
    }
}
