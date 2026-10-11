//! Telling a reply that reports a change from one that proposes it (R3-7).
//!
//! A heuristic over the reply's wording. It runs only when the run proposed
//! nothing, so a false positive costs one more model call, never a wrong write.

/// Phrases that report a change as made. A reply that proposes says so in other
/// words ("I've proposed", "waiting for your approval").
pub const CLAIMS: &[&str] = &[
    "done",
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

/// The first claim phrase in `reply`, if any.
pub fn claimed_change(reply: &str) -> Option<&'static str> {
    let reply = reply.to_lowercase();
    CLAIMS.iter().copied().find(|c| reply.contains(c))
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
        assert_eq!(claimed_change("Done ✅ — added it."), Some("done"));
        assert_eq!(
            claimed_change("I've created the note."),
            Some("i've created")
        );
        assert_eq!(
            claimed_change("I've proposed the note; it waits for you."),
            None
        );
    }
}
