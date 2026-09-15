//! `--bench-extraction`: role C1, scored against labels the corpus writes itself.
//!
//! Test scaffolding, on the same terms as [`super::ask`].
//!
//! What the numbers mean and what this instrument cannot see: `MODEL_BENCH.md`
//! Part 6.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use omni_me_core::extraction::{DocumentExtractor, ExtractionHint};
use omni_me_core::statement::StatementParse;
use omni_me_core::statement::parse::parse_brokerage_statement;
use rust_decimal::Decimal;

/// Where the paired corpus lives, overridable because it is not in the repo.
const CORPUS_ENV: &str = "OMNI_BENCH_CORPUS";
const DEFAULT_CORPUS: &str = ".reference/paisa-ledger";

/// How many cases one run scores. A month of statements is a large prompt, so
/// this is a spend control rather than a statistical one.
const DEFAULT_SAMPLE: usize = 12;
const SAMPLE_ENV: &str = "OMNI_BENCH_SAMPLE";

/// A directory of documents that are not transaction statements — the abstention
/// arm, whose honest answer is no postings at all.
///
/// Unset by default and named rather than discovered, because the directories
/// that hold such documents are named after institutions and this file is public.
/// A run without it says the arm was skipped rather than reporting a score that
/// quietly measured only the positive half.
const ABSENT_ENV: &str = "OMNI_BENCH_ABSENT";

/// How many absent documents one run scores.
const ABSENT_SAMPLE: usize = 4;

/// Rows above which a case is left out of the sample.
///
/// The extraction request sets no `max_tokens`, so the ceiling is whatever the
/// provider defaults to. A statement long enough to be truncated would score as
/// a recall failure by the model, which is the Stage 1 mistake in a new costume.
/// 40 rows is comfortably inside any default and still covers half the corpus.
const MAX_ROWS: usize = 40;

/// One month of one account: a CSV the parser turns into labels, and the PDFs
/// that state the same period for a person to read.
struct Case {
    /// The only identifier ever printed. Never the path — the directory names
    /// are institution names and the filenames carry account numbers, and a
    /// scorecard gets pasted into `MODEL_BENCH.md`, which is public.
    label: String,
    truth: StatementParse,
    /// PDF bytes paired with the anonymised tag that distinguishes them.
    documents: Vec<(String, PathBuf)>,
}

/// How one document scored against the labels its own period generated.
struct Scored {
    label: String,
    document: String,
    labelled: usize,
    returned: usize,
    /// Labelled amounts the model also returned, as a multiset intersection.
    matched: usize,
    /// Matches found only after ignoring the sign. A systematic sign flip is a
    /// different defect from a misread figure, and collapsing them hides which.
    sign_flipped: usize,
    /// Returned amounts with no labelled twin — the fabrication count.
    fabricated: usize,
    latency: Duration,
    error: Option<String>,
}

impl Scored {
    fn recall(&self) -> f64 {
        if self.labelled == 0 {
            // An empty statement period. Recall is undefined; fabrication is the
            // whole measurement, and `report` handles these separately.
            return 1.0;
        }
        self.matched as f64 / self.labelled as f64
    }
}

/// Stable short tag for a name that must not be printed.
///
/// FNV-1a, four hex digits. Deterministic so two runs of the same corpus produce
/// comparable scorecards, and meaningless to anyone without the corpus.
fn tag(name: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in name.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{:04x}", hash & 0xffff)
}

/// Walk `<account>/<year>/<year>-<month>/` for directories holding both a CSV
/// and at least one PDF.
///
/// Account directories are discovered rather than listed, so no institution name
/// is ever written into this file.
fn discover(corpus: &Path) -> Result<Vec<Case>, String> {
    let mut cases = Vec::new();
    let mut unparsed = 0usize;
    let mut too_long = 0usize;
    let mut incomplete = 0usize;

    let accounts = read_dirs(corpus)?;
    for account in accounts {
        let account_tag = tag(&file_name(&account));
        for year in read_dirs(&account)? {
            for month in read_dirs(&year)? {
                let (Some(csv), pdfs) =
                    (first_with_ext(&month, "csv"), all_with_ext(&month, "pdf"))
                else {
                    continue;
                };
                if pdfs.is_empty() {
                    continue;
                }
                let body = match std::fs::read_to_string(&csv) {
                    Ok(body) => body,
                    Err(_) => {
                        unparsed += 1;
                        continue;
                    }
                };
                let truth = match parse_brokerage_statement(&body) {
                    Ok(truth) => truth,
                    Err(_) => {
                        unparsed += 1;
                        continue;
                    }
                };
                // The parser's own rule, stated on `StatementParse::skipped`: a
                // parse with skipped lines is incomplete and must not be an oracle.
                if !truth.skipped.is_empty() {
                    incomplete += 1;
                    continue;
                }
                if truth.rows.len() > MAX_ROWS {
                    too_long += 1;
                    continue;
                }
                let documents = pdfs.into_iter().map(|p| (tag(&file_name(&p)), p)).collect();
                cases.push(Case {
                    label: format!("{account_tag}/{}", file_name(&month)),
                    truth,
                    documents,
                });
            }
        }
    }

    let empty = cases.iter().filter(|c| c.truth.rows.is_empty()).count();
    let documents: usize = cases.iter().map(|c| c.documents.len()).sum();
    println!(
        "corpus: {} usable cases ({empty} empty periods) over {documents} documents · \
         {incomplete} parsed incompletely · {too_long} over {MAX_ROWS} rows · {unparsed} unreadable",
        cases.len()
    );
    if cases.is_empty() {
        return Err(format!(
            "no usable cases under {} — set {CORPUS_ENV} to a corpus laid out as \
             <account>/<year>/<year>-<month>/",
            corpus.display()
        ));
    }
    // Sorted by the anonymised label, so the sample is the same set on every run
    // over the same corpus rather than filesystem order.
    cases.sort_by(|a, b| a.label.cmp(&b.label));
    Ok(cases)
}

/// Cases whose honest answer is no postings: real documents that carry no
/// transactions, read under the hint a bulk ingest would give them.
///
/// Not a trick question. `route_from_mime` returns `None` for every PDF because
/// a PDF could be a receipt, a paystub or a notice, so a document arriving with
/// nothing but bytes really is handed a hint that may not fit it. What is being
/// measured is whether a wrong hint produces an invented transaction.
fn discover_absent(dir: &Path) -> Vec<Case> {
    let mut found = Vec::new();
    collect_pdfs(dir, 0, &mut found);
    found.sort();
    // Spread across the tree rather than taking the first few, which in a corpus
    // filed by year would be one year's documents and one layout.
    let stride = (found.len() / ABSENT_SAMPLE.max(1)).max(1);
    found
        .into_iter()
        .step_by(stride)
        .take(ABSENT_SAMPLE)
        .map(|path| Case {
            label: format!("absent/{}", tag(&file_name(&path))),
            truth: StatementParse::default(),
            documents: vec![(tag(&file_name(&path)), path)],
        })
        .collect()
}

/// Depth cap on the absent-document walk. Corpora are filed by year or by year
/// and month; anything deeper is a layout this was not pointed at on purpose.
const MAX_WALK_DEPTH: usize = 3;

fn collect_pdfs(dir: &Path, depth: usize, into: &mut Vec<PathBuf>) {
    into.extend(with_ext(dir, "pdf"));
    if depth >= MAX_WALK_DEPTH {
        return;
    }
    for child in read_dirs(dir).unwrap_or_default() {
        collect_pdfs(&child, depth + 1, into);
    }
}

fn read_dirs(path: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = std::fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)))
        .collect();
    found.sort();
    found
}

fn first_with_ext(dir: &Path, ext: &str) -> Option<PathBuf> {
    with_ext(dir, ext).into_iter().next()
}

fn all_with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    with_ext(dir, ext)
}

/// Compare returned amounts against labelled ones as multisets.
///
/// Amounts rather than descriptions, because the two PDFs of one period describe
/// the same transaction differently — one says "Sams Food Fare" where the other
/// says "Withdrawal" — while both state the same figure. Dates cannot be compared
/// per row at all: `ExtractionResult` carries one date for the whole document.
fn score_amounts(labels: &[Decimal], returned: &[Decimal]) -> (usize, usize, usize) {
    let mut pool: BTreeMap<Decimal, usize> = BTreeMap::new();
    for amount in labels {
        *pool.entry(*amount).or_default() += 1;
    }
    let mut matched = 0;
    let mut unmatched = Vec::new();
    for amount in returned {
        match pool.get_mut(amount) {
            Some(count) if *count > 0 => {
                *count -= 1;
                matched += 1;
            }
            _ => unmatched.push(*amount),
        }
    }
    // Second pass over what is left, ignoring sign. A statement that splits
    // direction across debit and credit columns invites exactly this error.
    let mut sign_flipped = 0;
    let mut fabricated = 0;
    for amount in unmatched {
        match pool.get_mut(&-amount) {
            Some(count) if *count > 0 => {
                *count -= 1;
                sign_flipped += 1;
            }
            _ => fabricated += 1,
        }
    }
    (matched, sign_flipped, fabricated)
}

async fn score_one(
    extractor: &dyn DocumentExtractor,
    case: &Case,
    document: &str,
    path: &Path,
) -> Scored {
    let labels: Vec<Decimal> = case.truth.rows.iter().map(|r| r.amount).collect();
    let mut scored = Scored {
        label: case.label.clone(),
        document: document.to_string(),
        labelled: labels.len(),
        returned: 0,
        matched: 0,
        sign_flipped: 0,
        fabricated: 0,
        latency: Duration::ZERO,
        error: None,
    };

    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            scored.error = Some(e.to_string());
            return scored;
        }
    };

    let started = Instant::now();
    let result = extractor
        .extract(&bytes, "application/pdf", ExtractionHint::BankStatement)
        .await;
    scored.latency = started.elapsed();

    match result {
        Ok(extraction) => {
            let returned: Vec<Decimal> = extraction.postings.iter().map(|p| p.amount).collect();
            scored.returned = returned.len();
            let (matched, sign_flipped, fabricated) = score_amounts(&labels, &returned);
            scored.matched = matched;
            scored.sign_flipped = sign_flipped;
            scored.fabricated = fabricated;
        }
        Err(e) => scored.error = Some(e.to_string()),
    }
    scored
}

fn report(rows: &[Scored]) {
    println!();
    println!(
        "{:<18} {:<6} {:>5} {:>5} {:>6} {:>5} {:>5} {:>8}",
        "CASE", "DOC", "LBL", "RET", "MATCH", "SIGN", "FAB", "LATENCY"
    );
    for row in rows {
        let latency = format!("{:.1}s", row.latency.as_secs_f64());
        println!(
            "{:<18} {:<6} {:>5} {:>5} {:>5.0}% {:>5} {:>5} {:>8}  {}",
            row.label,
            row.document,
            row.labelled,
            row.returned,
            row.recall() * 100.0,
            row.sign_flipped,
            row.fabricated,
            latency,
            row.error.as_deref().unwrap_or(""),
        );
    }

    let errored = rows.iter().filter(|r| r.error.is_some()).count();
    let scored: Vec<&Scored> = rows.iter().filter(|r| r.error.is_none()).collect();
    if scored.is_empty() {
        println!("\nevery case errored — nothing was measured");
        return;
    }

    let empty: Vec<&&Scored> = scored.iter().filter(|r| r.labelled == 0).collect();
    let populated: Vec<&&Scored> = scored.iter().filter(|r| r.labelled > 0).collect();

    println!();
    if !populated.is_empty() {
        let recall: f64 =
            populated.iter().map(|r| r.recall()).sum::<f64>() / populated.len() as f64;
        let fabricated: usize = populated.iter().map(|r| r.fabricated).sum();
        let flipped: usize = populated.iter().map(|r| r.sign_flipped).sum();
        let labelled: usize = populated.iter().map(|r| r.labelled).sum();
        println!(
            "{} populated periods: mean recall {:.0}% · {flipped} sign-flipped · \
             {fabricated} fabricated against {labelled} labelled",
            populated.len(),
            recall * 100.0,
        );
    }
    // Reported apart from recall, which is undefined with nothing to recall.
    // A period with no transactions measures one thing only: whether the model
    // invents rows when the honest answer is none.
    if !empty.is_empty() {
        let clean = empty.iter().filter(|r| r.returned == 0).count();
        println!(
            "{} empty periods (abstention): {clean} answered with no postings, {} invented some",
            empty.len(),
            empty.len() - clean,
        );
    }
    let median = {
        let mut times: Vec<Duration> = scored.iter().map(|r| r.latency).collect();
        times.sort();
        times[times.len() / 2]
    };
    println!(
        "median latency {:.1}s · {errored} errored",
        median.as_secs_f64()
    );
}

pub async fn run(extractor: &dyn DocumentExtractor) {
    let corpus =
        PathBuf::from(std::env::var(CORPUS_ENV).unwrap_or_else(|_| DEFAULT_CORPUS.to_string()));
    let sample: usize = std::env::var(SAMPLE_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SAMPLE);

    // Discovery first, so a run with no endpoint configured is a usable dry run:
    // it proves the corpus parses into labels without spending a token.
    let cases = match discover(&corpus) {
        Ok(cases) => cases,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    // Spread the sample across accounts rather than taking the first N, which
    // would be one account's whole history and would measure one PDF layout.
    let stride = (cases.len() / sample.max(1)).max(1);
    let mut chosen: Vec<&Case> = cases.iter().step_by(stride).take(sample).collect();

    let absent = match std::env::var(ABSENT_ENV) {
        Ok(dir) => discover_absent(Path::new(&dir)),
        Err(_) => Vec::new(),
    };
    if absent.is_empty() {
        println!(
            "abstention arm SKIPPED — set {ABSENT_ENV} to a directory of documents that carry \
             no transactions. Without it this run scores only what the model finds, never what \
             it invents when there is nothing to find."
        );
    }
    chosen.extend(absent.iter());

    let requests: usize = chosen.iter().map(|c| c.documents.len()).sum();
    let labelled: usize = chosen.iter().map(|c| c.truth.rows.len()).sum();
    println!(
        "plan: {} cases · {requests} requests · {labelled} labelled rows · model {}",
        chosen.len(),
        extractor.name(),
    );
    for case in &chosen {
        println!(
            "  {:<18} {:>3} rows  {} documents",
            case.label,
            case.truth.rows.len(),
            case.documents.len()
        );
    }

    // A NullExtractor answers `Ok` with an empty draft rather than erroring, so
    // an unconfigured run would score as a model that found nothing at all.
    if extractor.name() == "null" {
        eprintln!(
            "no extractor configured — set [llm.extractor] with vision = true. \
             Refusing rather than scoring a NullExtractor's empty drafts as answers."
        );
        return;
    }

    let mut rows = Vec::new();
    for case in chosen {
        for (document, path) in &case.documents {
            rows.push(score_one(extractor, case, document, path).await);
        }
    }
    report(&rows);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(v: &str) -> Decimal {
        Decimal::from_str(v).unwrap()
    }

    #[test]
    fn an_exact_answer_matches_every_label() {
        let labels = vec![dec("9.11"), dec("-1.00"), dec("0.04")];
        assert_eq!(score_amounts(&labels, &labels.clone()), (3, 0, 0));
    }

    #[test]
    fn repeated_amounts_are_matched_as_a_multiset_not_a_set() {
        // Two identical cash-back rows are two transactions. Matching on a set
        // would score one returned row as covering both.
        let labels = vec![dec("0.04"), dec("0.04")];
        assert_eq!(score_amounts(&labels, &[dec("0.04")]), (1, 0, 0));
        assert_eq!(score_amounts(&labels, &labels.clone()), (2, 0, 0));
    }

    #[test]
    fn a_sign_flip_is_counted_apart_from_a_misread_figure() {
        // The debit/credit layout states 1.00 as a debit; the ledger convention
        // is -1.00. Reading the figure right and the direction wrong has to stay
        // visible as its own number.
        let labels = vec![dec("-1.00"), dec("-20.99")];
        assert_eq!(
            score_amounts(&labels, &[dec("1.00"), dec("20.99")]),
            (0, 2, 0)
        );
    }

    #[test]
    fn an_amount_with_no_label_is_fabricated() {
        let labels = vec![dec("9.11")];
        assert_eq!(
            score_amounts(&labels, &[dec("9.11"), dec("404.00")]),
            (1, 0, 1)
        );
    }

    #[test]
    fn the_running_balance_is_not_mistaken_for_a_transaction() {
        // A model that emits the closing balance as a posting is fabricating a
        // row, and the figure is unlike any transaction, so it must not match.
        let labels = vec![dec("9.11"), dec("-1.00")];
        assert_eq!(
            score_amounts(&labels, &[dec("9.11"), dec("-1.00"), dec("4514.63")]),
            (2, 0, 1)
        );
    }

    #[test]
    fn the_tag_is_stable_and_hides_the_name() {
        let first = tag("globepay_chequing");
        assert_eq!(
            first,
            tag("globepay_chequing"),
            "must be stable across runs"
        );
        assert_ne!(first, tag("ws_checking"));
        assert_eq!(first.len(), 4);
        assert!(!first.contains("globepay"));
    }
}
