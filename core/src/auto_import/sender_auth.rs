//! Reading the mailbox provider's own SPF / DKIM / DMARC verdict off a message.
//!
//! On a receipt draft a signal, never a gate (`docs/src/auto-import.md`). For a people link it is
//! the gate ruling 103 names (RULINGS.md), through [`vouches_for`].
//!
//! The header is the provider's, but its comments quote sender-chosen text (the envelope
//! address), so a verdict is read with comments removed, and every DMARC result must pass.

use std::fmt;

/// What the provider concluded about the sender of one message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SenderAuth {
    /// DMARC passed: the provider vouches for the From a person reads.
    Authenticated,
    /// DMARC failed, or with no DMARC every method reported failed. Worth
    /// surfacing in review.
    Unauthenticated,
    /// No verdict to read. A mailbox provider that stamps no header, or a
    /// message that reached the store by some other route.
    Unknown,
}

impl SenderAuth {
    /// Whether review should say something about this. `Unknown` must not warn:
    /// most mail would trip it, and a warning everything trips is noise.
    pub fn is_worth_flagging(self) -> bool {
        matches!(self, Self::Unauthenticated)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Authenticated => "authenticated",
            Self::Unauthenticated => "unauthenticated",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for SenderAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Read a verdict out of an `Authentication-Results` header value.
///
/// The caller must pass the topmost such header and nothing else. Anything
/// below it in the message can be forged by whoever sent it, which would turn
/// this signal into an attacker-controlled one.
pub fn verdict(header: Option<&str>) -> SenderAuth {
    let Some(header) = header.and_then(without_comments) else {
        return SenderAuth::Unknown;
    };
    // An injected pass cannot outvote the provider's own fail.
    let dmarc = results_for(&header, "dmarc");
    if !dmarc.is_empty() {
        return if dmarc.iter().all(|passed| *passed) {
            SenderAuth::Authenticated
        } else {
            SenderAuth::Unauthenticated
        };
    }
    // Without DMARC a DKIM or SPF pass vouches for the signer or the envelope,
    // which a spoofer can own; only "everything reported failed" says something.
    let others: Vec<bool> = ["dkim", "spf"]
        .iter()
        .flat_map(|method| results_for(&header, method))
        .collect();
    if !others.is_empty() && others.iter().all(|passed| !passed) {
        SenderAuth::Unauthenticated
    } else {
        SenderAuth::Unknown
    }
}

/// Whether the provider vouches for `address` itself: DMARC passed, and every `header.from` it
/// names is that address's domain. The gate for a confident people link (ruling 103).
/// It trusts the topmost header to be the provider's, true of Gmail; a provider that stamps none
/// leaves a sender's forged header on top, and this would believe it.
pub fn vouches_for(header: Option<&str>, address: &str) -> bool {
    let Some(domain) = address
        .rsplit_once('@')
        .map(|(_, d)| d.trim().to_ascii_lowercase())
    else {
        return false;
    };
    let Some(raw) = header else {
        return false;
    };
    if carries_structure(raw) {
        return false;
    }
    let Some(clean) = without_comments(raw) else {
        return false;
    };
    // Exactly one DMARC result, passing, naming this domain in its own entry.
    let dmarc: Vec<&str> = clean
        .split(';')
        .map(str::trim)
        .filter(|r| r.starts_with("dmarc"))
        .collect();
    let [entry] = dmarc.as_slice() else {
        return false;
    };
    let froms: Vec<&str> = entry
        .split_whitespace()
        .filter_map(|t| t.strip_prefix("header.from="))
        .collect();
    verdict(header) == SenderAuth::Authenticated
        && results_for(entry, "dmarc") == [true]
        && !froms.is_empty()
        && froms.iter().all(|f| *f == domain)
}

/// A quote or backslash anywhere, or a `;` inside a comment. The provider writes none of them,
/// but a quoted envelope address can carry all three, in a comment or in `smtp.mailfrom`, and
/// the gate fails closed on anything shaped like a breakout.
fn carries_structure(header: &str) -> bool {
    let mut depth: usize = 0;
    for c in header.chars() {
        match c {
            '"' | '\\' => return true,
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ';' if depth > 0 => return true,
            _ => {}
        }
    }
    false
}

/// The header lowercased with RFC 7601 comments removed, or `None` when its parentheses do not
/// balance: a sender-chosen `(` could otherwise hide the provider's result inside a comment.
fn without_comments(header: &str) -> Option<String> {
    let mut out = String::with_capacity(header.len());
    let mut depth: usize = 0;
    let mut chars = header.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' if depth > 0 => {
                chars.next();
            }
            '(' => {
                if depth == 0 {
                    out.push(' ');
                }
                depth += 1;
            }
            ')' => depth = depth.checked_sub(1)?,
            _ if depth == 0 => out.push(c.to_ascii_lowercase()),
            _ => {}
        }
    }
    (depth == 0).then_some(out)
}

/// Every result this method reported, `true` for a pass, in order.
///
/// RFC 7601 allows whitespace around the `=` and puts the result first, so the
/// value runs to the next whitespace or `;`.
fn results_for(header: &str, method: &str) -> Vec<bool> {
    let mut out = Vec::new();
    let mut rest = header;
    while let Some(at) = rest.find(method) {
        let (before, from_method) = rest.split_at(at);
        rest = &from_method[method.len()..];
        // A result starts after `;` (resinfo, RFC 7601). `header.d=dkim.example` contains
        // "dkim" and is not a verdict.
        let boundary_ok = before
            .trim_end()
            .chars()
            .next_back()
            .is_none_or(|c| c == ';');
        if !boundary_ok {
            continue;
        }
        let value = rest.trim_start();
        let Some(value) = value.strip_prefix('=') else {
            continue;
        };
        let value = value
            .trim_start()
            .split(|c: char| c.is_whitespace() || c == ';')
            .next()
            .unwrap_or("");
        if !value.is_empty() {
            out.push(value == "pass");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shaped after what Gmail actually writes, continuation lines included.
    const GMAIL_PASS: &str = "mx.google.com;\r\n       \
         dkim=pass header.i=@vendor.example header.s=s1 header.b=AbCd;\r\n       \
         spf=pass (google.com: domain of bounce@vendor.example designates \
         192.0.2.1 as permitted sender) smtp.mailfrom=bounce@vendor.example;\r\n       \
         dmarc=pass (p=REJECT sp=REJECT dis=NONE) header.from=vendor.example";

    #[test]
    fn a_passing_provider_verdict_reads_as_authenticated() {
        assert_eq!(verdict(Some(GMAIL_PASS)), SenderAuth::Authenticated);
    }

    #[test]
    fn a_failing_dmarc_reads_as_unauthenticated() {
        let header = "mx.google.com; dkim=none; spf=softfail; \
                      dmarc=fail (p=NONE sp=NONE dis=NONE) header.from=spoofed.example";
        assert_eq!(verdict(Some(header)), SenderAuth::Unauthenticated);
    }

    /// DMARC is strongest and answers alone. Plenty of legitimate mail fails SPF
    /// on a forwarding hop while DKIM survives it, and DMARC is what reconciles
    /// the two; reading SPF first would flag that mail wrongly.
    #[test]
    fn dmarc_outranks_a_failing_spf() {
        let header = "mx.google.com; spf=fail; dkim=pass header.i=@vendor.example; \
                      dmarc=pass header.from=vendor.example";
        assert_eq!(verdict(Some(header)), SenderAuth::Authenticated);
    }

    /// A spoofer can DKIM-sign with their own domain, so without DMARC a pass
    /// proves nothing about the From a person reads.
    #[test]
    fn without_dmarc_a_pass_is_unknown() {
        for header in [
            "mx.google.com; spf=fail; dkim=pass header.i=@attacker.example",
            "mx.google.com; spf=pass; dkim=fail",
            "mx.google.com; spf=pass smtp.mailfrom=v.example",
        ] {
            assert_eq!(verdict(Some(header)), SenderAuth::Unknown, "{header}");
        }
    }

    #[test]
    fn without_dmarc_everything_failing_is_unauthenticated() {
        assert_eq!(
            verdict(Some("mx.google.com; spf=fail; dkim=fail")),
            SenderAuth::Unauthenticated
        );
    }

    /// No header at all is the common case on a mailbox whose provider does not
    /// stamp one, and it must not read as a failure.
    #[test]
    fn an_absent_header_is_unknown_not_a_failure() {
        assert_eq!(verdict(None), SenderAuth::Unknown);
        assert!(!SenderAuth::Unknown.is_worth_flagging());
    }

    /// A header that names no method this reads is also unknown, not a failure.
    #[test]
    fn an_unreadable_header_is_unknown() {
        assert_eq!(verdict(Some("mx.google.com; none")), SenderAuth::Unknown);
        assert_eq!(verdict(Some("")), SenderAuth::Unknown);
    }

    /// The property that makes this parse safe to run over hostile text: a
    /// method name inside somebody's domain or selector is not a verdict.
    #[test]
    fn a_method_name_inside_another_token_is_not_a_verdict() {
        assert_eq!(
            verdict(Some("mx.google.com; header.d=dkim.spoofed.example")),
            SenderAuth::Unknown
        );
        assert_eq!(
            verdict(Some(
                "mx.google.com; header.i=@nodkim=pass.example; spf=fail"
            )),
            SenderAuth::Unauthenticated
        );
    }

    /// Only `pass` is a pass. `neutral`, `none`, `permerror` and the rest all
    /// mean the provider could not vouch for this sender.
    #[test]
    fn only_pass_counts_as_a_pass() {
        for result in ["neutral", "none", "permerror", "temperror", "policy"] {
            assert_eq!(
                verdict(Some(&format!("mx.google.com; dmarc={result}"))),
                SenderAuth::Unauthenticated,
                "dmarc={result} must not read as authenticated"
            );
        }
    }

    /// The envelope address is the sender's and Gmail quotes it in a comment, so a crafted
    /// quoted local part can close that comment early and plant a result of its own.
    #[test]
    fn a_result_planted_through_a_comment_never_reads_as_a_pass() {
        let planted = "mx.google.com; spf=pass (domain of \"x\");dmarc=pass;(\"@evil.example \
                       designates) smtp.mailfrom=evil.example; dmarc=fail header.from=v.example";
        assert_eq!(verdict(Some(planted)), SenderAuth::Unauthenticated);
        let swallowing = "mx.google.com; spf=pass (domain of \"x\");dmarc=pass;((\"@evil.example \
                          designates) smtp.mailfrom=evil.example; dmarc=fail header.from=v.example";
        assert_eq!(
            verdict(Some(swallowing)),
            SenderAuth::Unknown,
            "an unclosed comment would hide the real result"
        );
        assert_eq!(
            verdict(Some("mx.google.com; spf=fail (dmarc=pass)")),
            SenderAuth::Unauthenticated,
            "comment text is never a result"
        );
    }

    #[test]
    fn only_the_from_domain_dmarc_passed_for_is_vouched_for() {
        assert!(vouches_for(Some(GMAIL_PASS), "Billing@Vendor.example"));
        assert!(!vouches_for(Some(GMAIL_PASS), "someone@other.example"));
        assert!(!vouches_for(
            Some("mx.google.com; dmarc=pass"),
            "a@vendor.example"
        ));
        assert!(!vouches_for(None, "a@vendor.example"));
        assert!(
            !vouches_for(
                Some(
                    "mx.google.com; dmarc=pass header.from=vendor.example; \
                      dmarc=pass header.from=vendor.example"
                ),
                "a@vendor.example"
            ),
            "two DMARC results are one too many"
        );
        assert!(
            !vouches_for(
                Some(
                    "mx.google.com; spf=pass (domain of \"a b\"@x.example) \
                      smtp.mailfrom=x.example; dmarc=pass header.from=vendor.example"
                ),
                "a@vendor.example"
            ),
            "a quoted address in a comment fails closed"
        );
        assert!(
            !vouches_for(
                Some(
                    "mx.google.com; spf=pass \
                      smtp.mailfrom=\"x; dmarc=pass header.from=vendor.example ;\"@evil.example"
                ),
                "a@vendor.example"
            ),
            "a quoted address outside a comment fails closed too"
        );
    }

    #[test]
    fn whitespace_around_the_equals_is_tolerated() {
        assert_eq!(
            verdict(Some("mx.google.com; dmarc = pass header.from=v.example")),
            SenderAuth::Authenticated
        );
    }
}
