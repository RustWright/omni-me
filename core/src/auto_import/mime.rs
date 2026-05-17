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
        let bytes = att.contents().to_vec();
        attachments.push(MimeAttachment {
            filename,
            content_type,
            bytes,
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

    fn read_fixture(name: &str) -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".reference/imap poller")
            .join(name);
        std::fs::read(&path).expect(&format!("read fixture {path:?}"))
    }

    #[test]
    fn parses_sc_estatement_eml_with_pdf_attachment() {
        let bytes = read_fixture("Your Estatement on 30042026 now available.eml");
        let parsed = parse_eml(&bytes).expect("SC eml parses");
        assert!(
            parsed.from.contains("@sc.com")
                || parsed.from.contains("amazonses")
                || parsed.from.to_lowercase().contains("standard"),
            "unexpected From: {}",
            parsed.from
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
        assert!(pdf.bytes.starts_with(b"%PDF-"), "attachment bytes don't look like a PDF");
        eprintln!(
            "SC parse: from={}, subject={}, body {} chars, {} attachments, pdf {} bytes",
            parsed.from,
            parsed.subject,
            parsed.body_text.len(),
            parsed.attachments.len(),
            pdf.bytes.len()
        );
    }

    #[test]
    fn parses_inline_body_audible_eml() {
        let bytes = read_fixture("Thanks, your order is complete_audible.eml");
        let parsed = parse_eml(&bytes).expect("audible eml parses");
        assert!(
            parsed.from.to_lowercase().contains("audible"),
            "from: {}",
            parsed.from
        );
        assert!(
            !parsed.body_text.is_empty(),
            "audible body must yield text"
        );
        // Body should contain the order detail somewhere
        let body_lower = parsed.body_text.to_lowercase();
        assert!(
            body_lower.contains("audible") || body_lower.contains("order"),
            "audible body missing expected content"
        );
    }

    #[test]
    fn parses_oxio_invoice_eml() {
        let bytes = read_fixture("📫 oxio invoice available..eml");
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
            }],
        };
        assert!(parsed.find_attachment("application/pdf").is_some());
    }
}
