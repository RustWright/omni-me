//! MIME helpers — pull from / subject / date / body / attachments out of raw
//! `.eml` bytes (which is what IMAP returns).
//!
//! Thin wrapper over `mail-parser` so handlers don't import the crate
//! directly. Keeps the rest of `auto_import::` swappable on parser choice.

use chrono::{DateTime, Utc};
use mail_parser::{MessageParser, MimeHeaders};

#[derive(Debug, thiserror::Error)]
pub enum MimeError {
    #[error("failed to parse MIME message")]
    Parse,
    #[error("no usable text body found in message")]
    NoTextBody,
}

/// Single parsed MIME message — only the fields handlers actually use.
#[derive(Debug, Clone)]
pub struct ParsedMessage {
    pub from: String,
    pub subject: String,
    pub date: Option<DateTime<Utc>>,
    /// Plain-text view of the body. `mail-parser` falls back from text/plain
    /// to text/html (HTML-stripped) automatically.
    pub body_text: String,
    pub attachments: Vec<MimeAttachment>,
}

#[derive(Debug, Clone)]
pub struct MimeAttachment {
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
    /// True for a part the message body *renders* — a logo or banner carried by
    /// `Content-Disposition: inline` with a `Content-ID` the HTML references.
    ///
    /// ⚠️ **Not a formality.** A real bank statement email in the fixtures
    /// carries **five** inline JPEGs of branding beside its one real PDF, so
    /// anything treating every part as a file ends up handling ~173 KB of
    /// furniture per message. Kept as a flag rather than filtered here, because
    /// the two callers want different sets: the archive catalogues only real
    /// attachments, while text extraction should still read an oddly-inline PDF.
    pub is_inline: bool,
}

impl ParsedMessage {
    /// First attachment matching the given MIME prefix (e.g. `"application/pdf"`),
    /// case-insensitive. Returns `None` if no attachment matches.
    pub fn find_attachment(&self, mime_prefix: &str) -> Option<&MimeAttachment> {
        let needle = mime_prefix.to_ascii_lowercase();
        self.attachments
            .iter()
            .find(|a| a.content_type.to_ascii_lowercase().starts_with(&needle))
    }

    /// The parts a person would call attachments — what the archive catalogues.
    ///
    /// ⛔ Dropping the inline parts is **lossless here only because the raw
    /// message is archived whole**: the images are still inside that document,
    /// they simply do not each get a catalogue entry of their own. Filter these
    /// out anywhere the message itself is *not* kept and they are gone.
    pub fn real_attachments(&self) -> impl Iterator<Item = &MimeAttachment> {
        self.attachments.iter().filter(|a| !a.is_inline)
    }
}

/// Parse raw RFC 5322 bytes (what IMAP returns / `.eml` files store) into a
/// `ParsedMessage`.
pub fn parse_eml(bytes: &[u8]) -> Result<ParsedMessage, MimeError> {
    let parser = MessageParser::default();
    let msg = parser.parse(bytes).ok_or(MimeError::Parse)?;

    let from = msg
        .from()
        .and_then(|a| a.first())
        .and_then(|a| a.address())
        .unwrap_or("")
        .to_string();
    let subject = msg.subject().unwrap_or("").to_string();
    let date = msg.date().and_then(|d| {
        // mail-parser's DateTime → chrono via the raw RFC2822 string round-trip
        // (their DateTime is a different type).
        chrono::DateTime::parse_from_rfc2822(&d.to_rfc822())
            .ok()
            .map(|d| d.with_timezone(&Utc))
    });

    let body_text = msg
        .body_text(0)
        .map(|s| s.to_string())
        .or_else(|| {
            // Fall back to HTML view with tags stripped — mail-parser produces
            // a plain-text-ish HTML body via `body_html` → strip via a tiny
            // ad-hoc strip (avoids pulling in `ammonia` just for this).
            msg.body_html(0).map(|html| strip_html_tags(&html))
        })
        .unwrap_or_default();

    let mut attachments = Vec::new();
    for att in msg.attachments() {
        let filename = att
            .attachment_name()
            .unwrap_or("attachment.bin")
            .to_string();
        let content_type = att
            .content_type()
            .map(|ct| {
                let mut s = ct.ctype().to_string();
                if let Some(sub) = ct.subtype() {
                    s.push('/');
                    s.push_str(sub);
                }
                s
            })
            .unwrap_or_else(|| "application/octet-stream".to_string());
        // Two independent signals, ORed on purpose. `mail-parser` classifies a
        // cid-referenced image as `PartType::InlineBinary`, and the header can
        // also say so directly; senders are inconsistent about which they set,
        // and treating furniture as a document is the worse error of the two.
        let is_inline = matches!(att.body, mail_parser::PartType::InlineBinary(_))
            || att
                .content_disposition()
                .is_some_and(|disposition| disposition.is_inline());

        let bytes = att.contents().to_vec();
        attachments.push(MimeAttachment {
            filename,
            content_type,
            bytes,
            is_inline,
        });
    }

    Ok(ParsedMessage {
        from,
        subject,
        date,
        body_text,
        attachments,
    })
}

/// Minimal tag-stripper for fallback when text/plain is absent. Keeps text
/// nodes, drops tags. Not a sanitizer — handlers that pass output to LLM
/// don't care about XSS-safety, and we never render this HTML.
fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Read a `.reference/` fixture, or `None` if absent. `.reference/` is
    /// gitignored — fresh clones or CI without samples should skip these tests
    /// gracefully, not panic.
    fn read_fixture(name: &str) -> Option<Vec<u8>> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".reference/imap poller")
            .join(name);
        std::fs::read(&path).ok()
    }

    /// ⚠️ **The measurement this filter exists for**, pinned against a real
    /// bank email rather than asserted in the abstract: one PDF statement
    /// arrives alongside five inline JPEGs of branding. An archive that
    /// catalogued every part would file five logos as five documents per
    /// message, and the count is the only thing that makes that concrete.
    #[test]
    fn branding_images_are_inline_and_the_statement_is_not() {
        let bytes = match read_fixture("Your Estatement on 30042026 now available.eml") {
            Some(b) => b,
            None => {
                eprintln!("fixture missing — skipping");
                return;
            }
        };
        let parsed = parse_eml(&bytes).expect("SC eml parses");

        let inline = parsed.attachments.iter().filter(|a| a.is_inline).count();
        let real: Vec<&MimeAttachment> = parsed.real_attachments().collect();

        assert_eq!(inline, 5, "five cid-referenced JPEGs of branding");
        assert_eq!(
            real.len(),
            1,
            "exactly one thing a person would call a file"
        );
        assert!(real[0].content_type.starts_with("application/pdf"));
        assert!(
            parsed
                .attachments
                .iter()
                .filter(|a| a.is_inline)
                .all(|a| a.content_type.starts_with("image/")),
            "the filter must not be swallowing a real document"
        );
    }

    /// ⚠️ Three of the five real fixtures carry no attachment at all — the
    /// receipt is the body. This is why the email itself has to be archived:
    /// attachment-only archiving files nothing for these.
    #[test]
    fn an_inline_body_receipt_has_nothing_to_catalogue_but_itself() {
        for name in [
            "Thanks, your order is complete_audible.eml",
            "Your Walmart order was delivered.eml",
            "Manitoba Hydro Online Account - New Online Bill.eml",
        ] {
            let Some(bytes) = read_fixture(name) else {
                eprintln!("fixture {name} missing — skipping");
                continue;
            };
            let parsed = parse_eml(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(
                parsed.real_attachments().count(),
                0,
                "{name} has no real attachment"
            );
            assert!(
                !parsed.body_text.trim().is_empty(),
                "{name}: the body is the document, so it must not be empty"
            );
        }
    }

    #[test]
    fn parses_sc_estatement_eml_with_pdf_attachment() {
        let bytes = match read_fixture("Your Estatement on 30042026 now available.eml") {
            Some(b) => b,
            None => {
                eprintln!("fixture missing — skipping");
                return;
            }
        };
        let parsed = parse_eml(&bytes).expect("SC eml parses");
        // Institution-agnostic ON PURPOSE. This assertion used to list expected
        // sender domains, but the fixture in `.reference/` is a REAL email while
        // the expected domains were fictionalized to keep the institution out of
        // this public repo. The two could never agree: the test skipped on CI
        // (no fixture) and failed on any machine that had real samples, so it
        // sat red locally and green in CI.
        //
        // Naming the real domain here would fix the test by leaking exactly what
        // the fictionalization protects. The sender is not what this test is
        // about anyway — parsing an .eml and recovering its PDF is.
        assert!(
            parsed.from.contains('@'),
            "From header did not parse into an address"
        );
        assert!(
            parsed.subject.to_lowercase().contains("estatement")
                || parsed.subject.to_lowercase().contains("statement"),
            "subject: {}",
            parsed.subject
        );
        let pdf = parsed
            .find_attachment("application/pdf")
            .expect("SC eml should have application/pdf attachment");
        // PDFs always start with %PDF-
        assert!(
            pdf.bytes.starts_with(b"%PDF-"),
            "attachment bytes don't look like a PDF"
        );
        // Shapes only — no `from`, and no subject. This runs against a real
        // email, and test output is the kind of thing that gets pasted into an
        // issue or a log.
        eprintln!(
            "statement parse: body {} chars, {} attachments, pdf {} bytes",
            parsed.body_text.len(),
            parsed.attachments.len(),
            pdf.bytes.len()
        );
    }

    #[test]
    fn parses_inline_body_audible_eml() {
        let bytes = match read_fixture("Thanks, your order is complete_audible.eml") {
            Some(b) => b,
            None => {
                eprintln!("fixture missing — skipping");
                return;
            }
        };
        let parsed = parse_eml(&bytes).expect("audible eml parses");
        assert!(
            parsed.from.to_lowercase().contains("audible"),
            "from: {}",
            parsed.from
        );
        assert!(!parsed.body_text.is_empty(), "audible body must yield text");
        // Body should contain the order detail somewhere
        let body_lower = parsed.body_text.to_lowercase();
        assert!(
            body_lower.contains("audible") || body_lower.contains("order"),
            "audible body missing expected content"
        );
    }

    #[test]
    fn parses_oxio_invoice_eml() {
        let bytes = match read_fixture("📫 oxio invoice available..eml") {
            Some(b) => b,
            None => {
                eprintln!("fixture missing — skipping");
                return;
            }
        };
        let parsed = parse_eml(&bytes).expect("oxio eml parses");
        assert!(
            parsed.from.to_lowercase().contains("oxio"),
            "oxio from: {}",
            parsed.from
        );
        assert!(!parsed.body_text.is_empty());
    }

    #[test]
    fn strip_html_keeps_text_drops_tags() {
        let html = "<html><body><p>Hello <b>world</b></p></body></html>";
        let stripped = strip_html_tags(html);
        assert!(stripped.contains("Hello"));
        assert!(stripped.contains("world"));
        assert!(!stripped.contains('<'));
        assert!(!stripped.contains('>'));
    }

    #[test]
    fn find_attachment_is_case_insensitive() {
        let parsed = ParsedMessage {
            from: String::new(),
            subject: String::new(),
            date: None,
            body_text: String::new(),
            attachments: vec![MimeAttachment {
                filename: "x.pdf".into(),
                content_type: "Application/PDF".into(),
                bytes: vec![],
                is_inline: false,
            }],
        };
        assert!(parsed.find_attachment("application/pdf").is_some());
    }

    /// ⚠️ `find_attachment` deliberately still sees inline parts — it is what
    /// text extraction uses, and a sender that marks a real PDF `inline` should
    /// not make its contents unreadable. Only the archive's
    /// [`ParsedMessage::real_attachments`] filters them.
    #[test]
    fn the_two_attachment_views_differ_on_an_inline_part() {
        let parsed = ParsedMessage {
            from: String::new(),
            subject: String::new(),
            date: None,
            body_text: String::new(),
            attachments: vec![MimeAttachment {
                filename: "statement.pdf".into(),
                content_type: "application/pdf".into(),
                bytes: vec![],
                is_inline: true,
            }],
        };
        assert!(
            parsed.find_attachment("application/pdf").is_some(),
            "text extraction must still reach it"
        );
        assert_eq!(
            parsed.real_attachments().count(),
            0,
            "but the archive does not give it a catalogue entry of its own"
        );
    }
}
