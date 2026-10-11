//! Telling a reply that reports a change from one that proposes it (R3-7).
//!
//! A heuristic over the reply's wording. It runs only when the run proposed
//! nothing, so a false positive costs one more model call, never a wrong write.

/// Phrases that report a change as made. A reply that proposes says so in other
/// words ("I've proposed", "waiting for your approval").
/// Bare "done" is not one: "I didn't mean to imply I had done anything" is the
/// correction itself (bench, 2026-10-11).
pub const CLAIMS: &[&str] = &[
    "done ✅",
    "done!",
    "all done",
    "it's done",
    "that's done",
    "is done",
    "✅",
    "i've added",
    "i have added",
    "i've created",
    "i have created",
    "i've recorded",
    "i have recorded",
    "i've marked",
    "i have marked",
    "has been added",
    "has been created",
    "has been recorded",
    "is now recorded",
];

/// Phrases that announce a change about to be made. A reply ending on one with
/// nothing proposed stopped short: "Let me now propose the routine." was the
/// whole of a run's last word (bench, 2026-10-11).
pub const INTENTIONS: &[&str] = &[
    "let me now propose",
    "let me propose",
    "i'll propose",
    "i will propose",
    "i'll now propose",
    "let me create",
    "i'll create",
    "let me record",
    "i'll record",
    // Asking leave to propose, when the user already asked for the change: a
    // proposal is itself the question, answered in the inbox (S9, 2026-10-11:
    // "Shall I propose creating this as a new routine?").
    "shall i propose",
    "should i propose",
    "would you like me to propose",
    "want me to propose",
    "shall i create",
    "should i create",
    "would you like me to create",
    "shall i add",
    "would you like me to add",
    "shall i record",
];

/// The first claim phrase in `reply`, if any.
pub fn claimed_change(reply: &str) -> Option<&'static str> {
    let reply = reply.to_lowercase();
    CLAIMS.iter().copied().find(|c| reply.contains(c))
}

/// The first phrase announcing a change about to be made, if any.
pub fn announced_change(reply: &str) -> Option<&'static str> {
    let reply = reply.to_lowercase();
    INTENTIONS.iter().copied().find(|c| reply.contains(c))
}

/// What the model is told when it announced a change and then stopped.
pub fn follow_through(phrase: &str) -> String {
    format!(
        "You wrote {phrase:?}, but nothing was proposed, so the user has nothing to accept. \
         A proposal does not act on its own: the user accepts or declines it, so there is no \
         need to ask first. If they asked for the change, propose it now with the propose \
         tool; otherwise say plainly why you cannot."
    )
}

/// What the model is told when its reply claims a change it did not propose.
pub fn correction(phrase: &str) -> String {
    format!(
        "Your reply says {phrase:?}, but you proposed nothing in this run, so nothing has \
         changed. If the user asked for a change, propose it now. If not, rewrite your reply \
         so it does not say anything was done."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reported_change_is_caught_and_a_proposal_is_not() {
        assert_eq!(claimed_change("Done ✅ — added it."), Some("done ✅"));
        assert_eq!(
            claimed_change("You're right — I didn't mean to imply I had done anything."),
            None
        );
        assert_eq!(
            claimed_change("I've created the note."),
            Some("i've created")
        );
        assert_eq!(
            claimed_change("I've proposed the note; it waits for you."),
            None
        );
        assert_eq!(
            announced_change("Here is what I found. Let me now propose the routine."),
            Some("let me now propose")
        );
        assert_eq!(
            announced_change("Shall I propose creating this as a new routine in the app?"),
            Some("shall i propose")
        );
    }
}
