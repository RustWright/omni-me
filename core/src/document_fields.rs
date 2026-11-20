//! Deriving archive fields from a document, without a model where possible.
//!
//! Parser-sourced fields for statements, which `statement::parse` reads and knows what it checked;
//! a model handles the rest. `verified` means checked against the bank's own figures, not merely
//! parsed ([`DocumentField::verified`]): some exports parse cleanly declaring nothing.

use chrono::Utc;

use crate::events::{
    DOCUMENT_DATE_KEY, DOCUMENT_KIND_KEY, DOCUMENT_TAGS_KEY, DocumentField,
    DocumentFieldsExtractedPayload, Tag, encode_tag_set,
};
use crate::extraction::DocumentPart;
use crate::extraction::document::{self, DocumentReader};
use crate::statement::{StatementParse, Verifiability, parse};

/// Field keys this module emits, beside the three the projection hoists.
pub const PERIOD_START_KEY: &str = "period_start";
pub const PERIOD_END_KEY: &str = "period_end";
pub const CLOSING_BALANCE_KEY: &str = "closing_balance";
pub const ROW_COUNT_KEY: &str = "row_count";
pub const VERIFIABILITY_KEY: &str = "verifiability";

/// Which CSV export a file turned out to be.
///
/// The finance import path takes this from the user (`budget::import_chequing_csv`
/// has a `format` option). The archive cannot: a bulk-ingested file arrives with
/// nothing but bytes, so the format has to be *discovered* — see
/// [`claim_statement`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementFormat {
    Chequing,
    Brokerage,
    Transfer,
}

impl StatementFormat {
    pub const ALL: [StatementFormat; 3] = [
        StatementFormat::Chequing,
        StatementFormat::Brokerage,
        StatementFormat::Transfer,
    ];

    /// The `parser:<id>` form of [`DocumentField::source`].
    ///
    /// Names the *parser*, not the format, because the rank rule reads this
    /// prefix and a reader asking "why does this say 4,812.00" needs to know
    /// which code produced it.
    pub fn source(self) -> &'static str {
        match self {
            StatementFormat::Chequing => "parser:statement-chequing",
            StatementFormat::Brokerage => "parser:statement-brokerage",
            StatementFormat::Transfer => "parser:statement-transfer",
        }
    }

    /// The archive's `kind`, as the list view and every filter read it.
    pub fn kind(self) -> &'static str {
        match self {
            StatementFormat::Chequing => "bank_statement",
            StatementFormat::Brokerage => "brokerage_statement",
            StatementFormat::Transfer => "transfer_statement",
        }
    }

    fn parse(self, csv: &str) -> Result<StatementParse, String> {
        match self {
            StatementFormat::Chequing => parse::parse_chequing_statement(csv),
            StatementFormat::Brokerage => parse::parse_brokerage_statement(csv),
            StatementFormat::Transfer => parse::parse_transfer_statement(csv),
        }
    }

    /// Header columns that must all be present for this format to claim a
    /// file. Empty for the headerless format, which is why it is the fallback.
    ///
    /// Stricter than the parsers on purpose: their optional columns let one accept another's
    /// export, right when the user named the format. Tighten here, never the parser.
    fn signature(self) -> &'static [&'static str] {
        match self {
            StatementFormat::Brokerage => &["date", "amount", "balance"],
            StatementFormat::Transfer => &["date", "amount", "running balance", "transferwise id"],
            StatementFormat::Chequing => &[],
        }
    }

    fn header_matches(self, header: &[String]) -> bool {
        let sig = self.signature();
        !sig.is_empty()
            && sig
                .iter()
                .all(|want| header.iter().any(|h| h.eq_ignore_ascii_case(want)))
    }
}

/// Whether a parse is good enough to say this parser *recognised* the file,
/// as opposed to having chewed through it producing plausible wreckage.
///
/// Rows (a non-match parses empty), no skips (an unreadable transaction), and
/// `check_accounting` (an unclassified line), each catching what the others miss.
fn recognises(parse: &StatementParse) -> bool {
    !parse.rows.is_empty() && parse.skipped.is_empty() && parse.check_accounting().is_ok()
}

/// Find the one statement parser that recognises `csv`, if exactly one does.
///
/// Header signatures first; the headerless chequing map last, since it accepts any CSV with money
/// in the right columns. Ambiguity returns `None`, never the first match by declaration order;
/// the document then gets its fields from a model.
pub fn claim_statement(csv: &str) -> Option<(StatementFormat, StatementParse)> {
    let header = parse::header_columns(csv).unwrap_or_default();

    let mut claims: Vec<(StatementFormat, StatementParse)> = StatementFormat::ALL
        .into_iter()
        .filter(|format| format.header_matches(&header))
        .filter_map(|format| format.parse(csv).ok().map(|parse| (format, parse)))
        .filter(|(_, parse)| recognises(parse))
        .collect();

    match claims.len() {
        1 => return claims.pop(),
        0 => {}
        _ => {
            tracing::warn!(
                claimants = ?claims.iter().map(|(f, _)| f.source()).collect::<Vec<_>>(),
                "more than one statement signature matched a file; leaving its fields to a model"
            );
            return None;
        }
    }

    StatementFormat::Chequing
        .parse(csv)
        .ok()
        .filter(recognises)
        .map(|parse| (StatementFormat::Chequing, parse))
}

/// The `source` a person's own correction carries. Outranks every machine.
pub const HUMAN_SOURCE: &str = "human";

/// Turn one hand-entered value into the event that folds it onto a document.
///
/// `verified: true`, honest only because the person reading the document beside the field is the
/// oracle; never call it where the document cannot be shown. One key per event, so each
/// correction stays attributable.
pub fn human_correction(
    document_id: &str,
    key: &str,
    value: &str,
) -> DocumentFieldsExtractedPayload {
    DocumentFieldsExtractedPayload {
        document_id: document_id.to_string(),
        extracted_at: Utc::now().to_rfc3339(),
        fields: vec![DocumentField {
            key: key.to_string(),
            value: value.to_string(),
            source: HUMAN_SOURCE.to_string(),
            verified: true,
        }],
    }
}

/// A person's tag set for a document, as the one folded `tags` field.
///
/// The whole set every time, since the fold cannot remove a key. `verified: true` because a tag
/// is the person's own label, not a reading; that argument extends to no other key.
pub fn human_tag_set(document_id: &str, tags: &[Tag]) -> DocumentFieldsExtractedPayload {
    DocumentFieldsExtractedPayload {
        document_id: document_id.to_string(),
        extracted_at: Utc::now().to_rfc3339(),
        fields: vec![DocumentField {
            key: DOCUMENT_TAGS_KEY.to_string(),
            value: encode_tag_set(tags),
            source: HUMAN_SOURCE.to_string(),
            verified: true,
        }],
    }
}

/// An email's own date, from its `Date:` header, at parser rank: above the
/// reader's guess, below a person's correction. Retention counts from it, so a
/// backfill of old mail lands already as old as it is.
pub fn email_date(
    document_id: &str,
    sent: chrono::DateTime<Utc>,
) -> DocumentFieldsExtractedPayload {
    DocumentFieldsExtractedPayload {
        document_id: document_id.to_string(),
        extracted_at: Utc::now().to_rfc3339(),
        fields: vec![DocumentField {
            key: crate::events::DOCUMENT_DATE_KEY.to_string(),
            value: sent.date_naive().to_string(),
            source: "parser:email".to_string(),
            verified: false,
        }],
    }
}

/// The deterministic half: what a parser can say about the bytes alone.
///
/// Synchronous with no reader, so "no model at ingest" is structural. No MIME gate: bulk ingest
/// mislabels files, valid UTF-8 already excludes PDFs and images, and [`claim_statement`] is the
/// real filter.
pub fn parser_fields(document_id: &str, bytes: &[u8]) -> Option<DocumentFieldsExtractedPayload> {
    let text = std::str::from_utf8(bytes).ok()?;
    let (format, parse) = claim_statement(text)?;
    Some(fields_from_statement(document_id, format, &parse))
}

/// Derive a document's fields, preferring the answer that can be checked.
///
/// Parser first, model only on refusal, as [`DocumentField::rank`] orders them. The scheduled
/// path; ingest calls [`parser_fields`], keeping the network out of filing. `None` is a normal
/// outcome: the document is archived and findable anyway.
pub async fn derive_fields(
    document_id: &str,
    bytes: &[u8],
    mime: &str,
    reader: Option<&dyn DocumentReader>,
    tags_in_use: &[String],
) -> Option<DocumentFieldsExtractedPayload> {
    if let Some(mut parsed) = parser_fields(document_id, bytes) {
        // Everything the parser recognises is a statement. Written here so a
        // re-read for a missing category (the enrichment sweep) settles it rather
        // than finding it missing again on every tick.
        parsed.fields.push(crate::events::DocumentField {
            key: crate::events::DOCUMENT_RETENTION_KEY.to_string(),
            value: "statement".to_string(),
            source: "parser:statement".to_string(),
            verified: false,
        });
        return Some(parsed);
    }

    let reader = reader?;
    match reader
        .read_document_among(&[DocumentPart::new(bytes, mime)], tags_in_use)
        .await
    {
        Ok(summary) => Some(document::to_fields_payload(document_id, &summary)),
        Err(e) => {
            // Skip, never propagate. A model that was slow, refused the MIME, or
            // answered off-schema leaves the document exactly as it was —
            // archived, searchable by name, and re-readable later. Turning that
            // into an error would make a backfill abort partway through a corpus
            // over one awkward file.
            tracing::warn!(
                document_id,
                reader = reader.name(),
                error = %e,
                "no fields could be read from a document"
            );
            None
        }
    }
}

/// The fields a recognised statement yields, ready to append as an event.
///
/// `verified` asks [`StatementParse::verifiability`], since empty `import_blockers` also means "no
/// checks". It covers what the balance walk confirms (period, row count, closing balance), never
/// `kind` or `verifiability`, which nothing measured.
pub fn fields_from_statement(
    document_id: &str,
    format: StatementFormat,
    parse: &StatementParse,
) -> DocumentFieldsExtractedPayload {
    let verifiability = parse.verifiability();
    let checked =
        parse.import_blockers().is_empty() && verifiability != Verifiability::NotVerifiable;

    let source = format.source();
    let field = |key: &str, value: String, verified: bool| DocumentField {
        key: key.to_string(),
        value,
        source: source.to_string(),
        verified,
    };

    let mut fields = vec![field(DOCUMENT_KIND_KEY, format.kind().to_string(), false)];

    // Rows are not guaranteed sorted, and a statement occasionally carries a few
    // days either side of its nominal period — so the bounds come from the rows
    // themselves rather than from the first and last of them.
    let first = parse.rows.iter().map(|r| r.date).min();
    let last = parse.rows.iter().map(|r| r.date).max();
    if let Some(start) = first {
        fields.push(field(PERIOD_START_KEY, start.to_string(), checked));
    }
    if let Some(end) = last {
        fields.push(field(PERIOD_END_KEY, end.to_string(), checked));
        // The closing date is what a date filter over the archive should match:
        // a statement is *about* the period it closes.
        fields.push(field(DOCUMENT_DATE_KEY, end.to_string(), checked));
    }

    fields.push(field(ROW_COUNT_KEY, parse.rows.len().to_string(), checked));

    if let Some(balance) = parse.closing_balance() {
        fields.push(field(CLOSING_BALANCE_KEY, balance.to_string(), checked));
    }

    // The token, not `Verifiability::describe`. That renders a sentence for a
    // person to read, which belongs to whichever surface is showing this — a
    // stored value has to be filterable, and a stored sentence would also freeze
    // today's wording into every document's history.
    fields.push(field(
        VERIFIABILITY_KEY,
        verifiability_token(verifiability).to_string(),
        false,
    ));

    DocumentFieldsExtractedPayload {
        document_id: document_id.to_string(),
        extracted_at: Utc::now().to_rfc3339(),
        fields,
    }
}

fn verifiability_token(v: Verifiability) -> &'static str {
    match v {
        Verifiability::AgainstDeclaredFigures => "against_declared_figures",
        Verifiability::AgainstOwnRunningBalance => "against_own_running_balance",
        Verifiability::NotVerifiable => "not_verifiable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Records whether it was asked, so "the parser wins" can be tested as
    /// *the model was never consulted* rather than as "the output looks parsed".
    struct StubReader {
        asked: AtomicBool,
    }

    impl StubReader {
        fn new() -> Self {
            Self {
                asked: AtomicBool::new(false),
            }
        }
    }

    #[async_trait::async_trait]
    impl DocumentReader for StubReader {
        fn name(&self) -> &str {
            "stub"
        }

        async fn read_document(
            &self,
            _parts: &[DocumentPart<'_>],
        ) -> Result<document::DocumentSummary, crate::extraction::ExtractionError> {
            self.asked.store(true, Ordering::SeqCst);
            document::parse_summary(
                serde_json::json!({ "kind": "letter", "title": "A letter" }),
                "stub",
            )
        }
    }

    /// The brokerage export's shape, running balance included.
    const BROKERAGE: &str = "\
Date,Description,Amount,Balance
2024-01-05,Dividend ACME,10.00,110.00
2024-01-20,Buy ACME,-50.00,60.00
";

    #[test]
    fn a_correction_outranks_every_machine_source_and_is_verified() {
        let payload = human_correction("doc-9", DOCUMENT_KIND_KEY, "lease");
        let field = &payload.fields[0];

        assert_eq!(field.source, "human");
        assert!(
            field.verified,
            "the person read the document beside the field — that is the oracle"
        );
        assert!(
            DocumentField::rank(&field.source)
                > DocumentField::rank(StatementFormat::Chequing.source()),
            "⛔ a re-run parser must never undo a correction"
        );
        assert_eq!(payload.fields.len(), 1, "one key per correction event");
    }

    #[test]
    fn a_recognised_statement_yields_parser_sourced_fields() {
        let (format, parse) = claim_statement(BROKERAGE).expect("brokerage export must be claimed");
        assert_eq!(format, StatementFormat::Brokerage);

        let payload = fields_from_statement("doc-1", format, &parse);
        let get = |k: &str| {
            payload
                .fields
                .iter()
                .find(|f| f.key == k)
                .unwrap_or_else(|| panic!("missing field {k}"))
        };

        assert_eq!(get(DOCUMENT_KIND_KEY).value, "brokerage_statement");
        assert_eq!(get(PERIOD_START_KEY).value, "2024-01-05");
        assert_eq!(get(PERIOD_END_KEY).value, "2024-01-20");
        assert_eq!(
            get(DOCUMENT_DATE_KEY).value,
            "2024-01-20",
            "a statement is dated by the period it closes"
        );
        assert_eq!(get(ROW_COUNT_KEY).value, "2");
        assert!(
            payload
                .fields
                .iter()
                .all(|f| f.source.starts_with("parser:")),
            "every field here outranks a model's, and the prefix is what says so"
        );
    }

    #[test]
    fn a_running_balance_is_an_oracle_and_the_fields_it_covers_are_verified() {
        let (format, parse) = claim_statement(BROKERAGE).unwrap();
        let payload = fields_from_statement("doc-1", format, &parse);
        let get = |k: &str| payload.fields.iter().find(|f| f.key == k).unwrap();

        assert_eq!(
            parse.verifiability(),
            Verifiability::AgainstOwnRunningBalance
        );
        assert!(
            get(CLOSING_BALANCE_KEY).verified,
            "the chain walks, so the balance was checked against the bank's own figures"
        );
        assert!(get(ROW_COUNT_KEY).verified);
        assert!(
            !get(DOCUMENT_KIND_KEY).verified,
            "⛔ classification is this parser's opinion — nothing measured it"
        );
        assert!(
            !get(VERIFIABILITY_KEY).verified,
            "⛔ a description of the check is not itself checked"
        );
    }

    #[test]
    fn a_clean_parse_of_a_format_that_declares_nothing_is_not_verified() {
        // The case the whole `verified` flag exists for. This export carries
        // no balance column, so every line parses and nothing is checked —
        // `import_blockers` is empty because there was no gate to fail.
        const NO_BALANCE: &str = "\
Date,Description,Amount
2024-01-05,Dividend ACME,10.00
2024-01-20,Buy ACME,-50.00
";
        let (format, parse) = claim_statement(NO_BALANCE)
            .expect("the headerless chequing map reads this positionally");
        assert_eq!(format, StatementFormat::Chequing);
        assert_eq!(parse.verifiability(), Verifiability::NotVerifiable);
        assert!(parse.import_blockers().is_empty(), "nothing could fail");

        let payload = fields_from_statement("doc-2", format, &parse);
        assert!(
            payload.fields.iter().all(|f| !f.verified),
            "⛔ a clean parse is not a checked parse, and collapsing the two is \
             how an unverified import comes to read as a verified one"
        );
    }

    #[tokio::test]
    async fn a_parseable_statement_never_reaches_the_model() {
        let reader = StubReader::new();
        let payload = derive_fields(
            "doc-1",
            BROKERAGE.as_bytes(),
            "text/csv",
            Some(&reader),
            &[],
        )
        .await
        .unwrap();

        assert!(
            !reader.asked.load(Ordering::SeqCst),
            "asking a model about a file a parser can read buys a value that \
             loses the fold anyway"
        );
        assert!(
            payload
                .fields
                .iter()
                .all(|f| f.source.starts_with("parser:"))
        );
    }

    #[tokio::test]
    async fn a_file_no_parser_recognises_falls_to_the_model() {
        let reader = StubReader::new();
        let payload = derive_fields(
            "doc-2",
            b"a letter, not a statement",
            "text/plain",
            Some(&reader),
            &[],
        )
        .await
        .unwrap();

        assert!(reader.asked.load(Ordering::SeqCst));
        assert_eq!(payload.fields[0].value, "letter");
        assert!(payload.fields.iter().all(|f| !f.verified));
    }

    #[tokio::test]
    async fn with_no_reader_configured_a_document_simply_gains_no_fields() {
        // Not an error. The document is already archived and findable by name;
        // a backfill must not abort over a file nothing can structure.
        assert!(
            derive_fields("doc-3", b"a letter", "text/plain", None, &[])
                .await
                .is_none()
        );
    }

    #[test]
    fn a_file_no_parser_recognises_is_left_to_a_model() {
        assert!(claim_statement("this is not a statement at all\n").is_none());
        assert!(claim_statement("").is_none());
    }

    #[test]
    fn the_chequing_map_does_not_claim_a_brokerage_export() {
        // The discovery rule's real hazard, and the reason `recognises`
        // insists on an empty skip ledger. The chequing map is positional and
        // headerless, so it will read any four-column CSV — here it lands on
        // Amount and Balance as debit and credit. `read_amount` refuses a row
        // with both populated rather than netting them, which turns every row
        // into a skip and disqualifies the claim. Netting would instead have
        // produced a confident wrong answer with a parser source on it.
        let chequing = StatementFormat::Chequing.parse(BROKERAGE).unwrap();
        assert!(
            !chequing.skipped.is_empty(),
            "the misread has to be visible, not silent"
        );
        assert!(!recognises(&chequing));
    }

    #[test]
    fn a_lenient_parser_does_not_out_claim_the_format_that_declares_itself() {
        // The overlap that made the first version of this rule useless. The
        // transfer map takes `running balance` and `transferwise id` as
        // *optional*, so it parses a brokerage export happily — two claimants,
        // and a rule that refuses ambiguity then refused every real file. The
        // signature test is what separates them, and it lives here rather than
        // in the parser because the import path depends on that leniency.
        let transfer = StatementFormat::Transfer.parse(BROKERAGE);
        assert!(
            transfer.is_ok_and(|p| recognises(&p)),
            "the parser itself still accepts it — that is the hazard, not a bug"
        );
        assert!(
            !StatementFormat::Transfer.header_matches(&parse::header_columns(BROKERAGE).unwrap())
        );

        let (format, _) = claim_statement(BROKERAGE).unwrap();
        assert_eq!(format, StatementFormat::Brokerage);
    }
}
