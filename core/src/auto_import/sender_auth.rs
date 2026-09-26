//! Reading the mailbox provider's own SPF / DKIM / DMARC verdict off a message.
//!
//! A signal on a draft, never a gate. Rationale, and why no draft is ever
//! dropped for failing it: `docs/src/auto-import.md`.

use std::fmt;

/// What the provider concluded about the sender of one message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SenderAuth {
    /// At least one of DMARC, DKIM or SPF passed.
    Authenticated,
    /// The provider checked and nothing passed. Worth surfacing in review.
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

/// The methods this reads, strongest first. DMARC subsumes the other two, so a
/// DMARC verdict is taken alone when present.
const METHODS: [&str; 3] = ["dmarc", "dkim", "spf"];

/// Read a verdict out of an `Authentication-Results` header value.
///
/// The caller must pass the topmost such header and nothing else. Anything
/// below it in the message can be forged by whoever sent it, which would turn
/// this signal into an attacker-controlled one.
pub fn verdict(header: Option<&str>) -> SenderAuth {
    let Some(header) = header else {
        return SenderAuth::Unknown;
    };
    let lower = header.to_ascii_lowercase();
    for method in METHODS {
        match result_for(&lower, method) {
            Some(true) => return SenderAuth::Authenticated,
            Some(false) => return SenderAuth::Unauthenticated,
            None => continue,
        }
    }
    SenderAuth::Unknown
}

/// `Some(passed)` when this method reported at all, `None` when it is absent.
///
/// RFC 7601 allows whitespace around the `=` and puts the result first, so the
/// value runs to the next whitespace, `;` or `(`.
fn result_for(lower_header: &str, method: &str) -> Option<bool> {
    let mut rest = lower_header;
    while let Some(at) = rest.find(method) {
        let (before, from_method) = rest.split_at(at);
        rest = &from_method[method.len()..];
        // `header.d=dkim.example` contains "dkim" and is not a verdict. A real
        // method name is preceded by a delimiter, not by part of a token.
        let boundary_ok = before
            .chars()
            .next_back()
            .is_none_or(|c| c == ';' || c.is_whitespace());
        if !boundary_ok {
            continue;
        }
        let value = rest.trim_start();
        let Some(value) = value.strip_prefix('=') else {
            continue;
        };
        let value = value
            .trim_start()
            .split(|c: char| c.is_whitespace() || c == ';' || c == '(')
            .next()
            .unwrap_or("");
        if value.is_empty() {
            continue;
        }
        return Some(value == "pass");
    }
    None
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

    #[test]
    fn dkim_answers_when_dmarc_is_absent() {
        assert_eq!(
            verdict(Some(
                "mx.google.com; spf=fail; dkim=pass header.i=@v.example"
            )),
            SenderAuth::Authenticated
        );
        assert_eq!(
            verdict(Some("mx.google.com; spf=pass; dkim=fail")),
            SenderAuth::Unauthenticated
        );
    }

    #[test]
    fn spf_alone_still_answers() {
        assert_eq!(
            verdict(Some("mx.google.com; spf=pass smtp.mailfrom=v.example")),
            SenderAuth::Authenticated
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

    #[test]
    fn whitespace_around_the_equals_is_tolerated() {
        assert_eq!(
            verdict(Some("mx.google.com; dmarc = pass header.from=v.example")),
            SenderAuth::Authenticated
        );
    }
}
