//! The card that settles one assistant proposal.
//!
//! ⚠️ **Shared rather than copied, because two screens now review proposals.**
//! The assistant inbox shows everything without a domain screen of its own;
//! Finances shows the ledger's, beside the ledger. ⛔ A second renderer would be
//! a second place for the irreversibility warning and the evidence row to drift,
//! and those two are the parts of the card that make an approval informed.

use dioxus::prelude::*;

use crate::bridge;
use crate::components::primitives::{Button, ButtonSize, ButtonVariant};
use crate::types::AssistantProposal;

/// One proposal, with the two buttons that settle it.
///
/// ⚠️ Both buttons disable while the decision is in flight. Approving twice is
/// refused by `core` — but the refusal arrives as an error the user did not
/// cause, and a double-tap on a phone is the ordinary way to produce one.
#[component]
pub fn ProposalCard(proposal: AssistantProposal, on_decided: EventHandler<()>) -> Element {
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
