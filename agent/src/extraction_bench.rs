//! `--bench-extraction`: role C1, scored against labels the corpus writes itself.
//!
//! Test scaffolding, on the same terms as [`super::ask`].
//!
//! What the numbers mean and what this instrument cannot see: `MODEL_BENCH.md`
//! Part 6.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, Instant};

use omni_me_core::extraction::{
    DEFAULT_CONFIDENCE_THRESHOLD, DocumentExtractor, DocumentPart, ExtractionHint, verify,
};
use omni_me_core::llm::Sampling;
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

/// How many times each document is read.
///
/// Three, matching `--bench-reading`, because one is not enough to rank this seat:
/// the same model on the same 39-row statement returned 13 sign-flips on one run
/// and 0 on the next (`MODEL_BENCH.md` R26). It multiplies the spend, which is why
/// it is overridable — but a single-shot run now says its stability is unmeasured
/// rather than printing a number that looks like a ranking.
const REPEATS_ENV: &str = "OMNI_BENCH_EXTRACT_REPEATS";
const DEFAULT_REPEATS: usize = 3;

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

/// One call's numbers, before they are pooled across the repeats.
struct Run {
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

/// How one document scored across every run of it.
struct Scored {
    label: String,
    document: String,
    /// Labels this document has, per run. Not pooled: the column reads as the
    /// statement's own row count, which `repeats` must not multiply.
    labelled: usize,
    runs: usize,
    errors: usize,
    /// Summed over the runs that answered.
    returned: usize,
    matched: usize,
    sign_flipped: usize,
    fabricated: usize,
    /// Whether every run that answered scored identically.
    ///
    /// The column this arm was missing. The same model on the same 39-row
    /// statement returned 13 sign-flips on one run and 0 on the next, and a
    /// single-shot arm reports whichever it drew as the seat's number.
    agreed: bool,
    /// Median over the runs that answered. An errored run's latency is left out:
    /// a refused request is often fast, and would drag the median toward a
    /// number no successful call ever took.
    latency: Duration,
    first_error: Option<String>,
}

impl Scored {
    /// Runs that produced a score at all.
    fn answered(&self) -> usize {
        self.runs.saturating_sub(self.errors)
    }

    fn recall(&self) -> f64 {
        let asked = self.labelled * self.answered();
        if asked == 0 {
            // An empty statement period, or a document every run refused. Recall
            // is undefined either way; `report` handles both separately.
            return 1.0;
        }
        self.matched as f64 / asked as f64
    }
}

/// Pool one document's runs into the row that gets printed.
fn summarise(label: &str, document: &str, labelled: usize, runs: Vec<Run>) -> Scored {
    let mut s = Scored {
        label: label.to_string(),
        document: document.to_string(),
        labelled,
        runs: runs.len(),
        errors: 0,
        returned: 0,
        matched: 0,
        sign_flipped: 0,
        fabricated: 0,
        agreed: true,
        latency: Duration::ZERO,
        first_error: None,
    };
    let mut latencies = Vec::new();
    let mut scores = Vec::new();

    for run in &runs {
        if let Some(e) = &run.error {
            s.errors += 1;
            if s.first_error.is_none() {
                s.first_error = Some(e.clone());
            }
            continue;
        }
        latencies.push(run.latency);
        s.returned += run.returned;
        s.matched += run.matched;
        s.sign_flipped += run.sign_flipped;
        s.fabricated += run.fabricated;
        scores.push((run.returned, run.matched, run.sign_flipped, run.fabricated));
    }

    s.agreed = scores.windows(2).all(|w| w[0] == w[1]);
    latencies.sort_unstable();
    s.latency = latencies
        .get(latencies.len() / 2)
        .copied()
        .unwrap_or(Duration::ZERO);
    s
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
    collect_documents(dir, &["pdf"], 0, &mut found);
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

fn collect_documents(dir: &Path, exts: &[&str], depth: usize, into: &mut Vec<PathBuf>) {
    for ext in exts {
        into.extend(with_ext(dir, ext));
    }
    if depth >= MAX_WALK_DEPTH {
        return;
    }
    for child in read_dirs(dir).unwrap_or_default() {
        collect_documents(&child, exts, depth + 1, into);
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

async fn score_one(extractor: &dyn DocumentExtractor, labels: &[Decimal], path: &Path) -> Run {
    let mut scored = Run {
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
        .extract(
            &[DocumentPart::new(&bytes, "application/pdf")],
            ExtractionHint::BankStatement,
        )
        .await;
    scored.latency = started.elapsed();

    match result {
        Ok(extraction) => {
            let returned: Vec<Decimal> = extraction.postings.iter().map(|p| p.amount).collect();
            scored.returned = returned.len();
            let (matched, sign_flipped, fabricated) = score_amounts(labels, &returned);
            scored.matched = matched;
            scored.sign_flipped = sign_flipped;
            scored.fabricated = fabricated;
        }
        Err(e) => scored.error = Some(e.to_string()),
    }
    scored
}

fn report(rows: &[Scored], model: &str, repeats: usize, sampling: Sampling) {
    println!(
        "\nrole C1 — statement extraction · model {model} · {repeats} runs per document · \
         sampling {}\n",
        sampling.describe()
    );
    println!(
        "{:<18} {:<6} {:>5} {:>5} {:>6} {:>5} {:>5} {:>4} {:>6} {:>8}",
        "CASE", "DOC", "LBL", "RET", "MATCH", "SIGN", "FAB", "ERR", "AGREE", "MEDIAN"
    );
    for row in rows {
        let latency = format!("{:.1}s", row.latency.as_secs_f64());
        // Two runs is the least that can disagree, so one is "not asked" rather
        // than agreement — the distinction a bare `yes` would erase.
        let agree = match (row.answered(), row.agreed) {
            (0 | 1, _) => "n/a",
            (_, true) => "yes",
            (_, false) => "NO",
        };
        println!(
            "{:<18} {:<6} {:>5} {:>5} {:>5.0}% {:>5} {:>5} {:>4} {:>6} {:>8}  {}",
            row.label,
            row.document,
            row.labelled,
            // Per run, so it compares against LBL rather than against LBL x runs.
            mean_per_run(row.returned, row.answered()),
            row.recall() * 100.0,
            row.sign_flipped,
            row.fabricated,
            row.errors,
            agree,
            latency,
            row.first_error.as_deref().unwrap_or(""),
        );
    }
    println!(
        "  LBL and RET are per run; MATCH is pooled; SIGN, FAB and ERR are summed \
         over {repeats} runs."
    );

    let errored = rows.iter().filter(|r| r.answered() == 0).count();
    let scored: Vec<&Scored> = rows.iter().filter(|r| r.answered() > 0).collect();
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
             {fabricated} fabricated against {labelled} labelled rows per run",
            populated.len(),
            recall * 100.0,
        );
        // The R26 column, said in words: a seat cannot be ranked on keys that
        // move between runs of the same document.
        let unstable = populated.iter().filter(|r| !r.agreed).count();
        let comparable = populated.iter().filter(|r| r.answered() > 1).count();
        if comparable == 0 {
            println!(
                "  stability UNMEASURED — one run per document. Set {REPEATS_ENV} to at \
                 least 2 before ranking anything on these numbers."
            );
        } else {
            println!(
                "  {unstable} of {comparable} documents scored differently between runs \
                 of the same work"
            );
        }
    }
    // Reported apart from recall, which is undefined with nothing to recall.
    // A period with no transactions measures one thing only: whether the model
    // invents rows when the honest answer is none.
    if !empty.is_empty() {
        let clean = empty.iter().filter(|r| r.returned == 0).count();
        println!(
            "{} empty periods (abstention): {clean} answered with no postings on every run, \
             {} invented some",
            empty.len(),
            empty.len() - clean,
        );
    }
    let median = {
        let mut times: Vec<Duration> = scored.iter().map(|r| r.latency).collect();
        times.sort();
        times[times.len() / 2]
    };
    let errored_runs: usize = rows.iter().map(|r| r.errors).sum();
    println!(
        "median latency {:.1}s · {errored} documents errored on every run · \
         {errored_runs} errored runs in total",
        median.as_secs_f64()
    );
}

/// A pooled count back to per-run, for a column that sits beside a per-run one.
fn mean_per_run(total: usize, runs: usize) -> String {
    if runs == 0 {
        return "—".to_string();
    }
    let mean = total as f64 / runs as f64;
    // One decimal only when the runs actually disagreed, so a stable row stays
    // as readable as it was before repeats existed.
    if (mean - mean.round()).abs() < f64::EPSILON {
        format!("{}", mean.round() as usize)
    } else {
        format!("{mean:.1}")
    }
}

pub async fn run(extractor: &dyn DocumentExtractor, sampling: Sampling) {
    let corpus =
        PathBuf::from(std::env::var(CORPUS_ENV).unwrap_or_else(|_| DEFAULT_CORPUS.to_string()));
    let sample: usize = std::env::var(SAMPLE_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SAMPLE);
    let repeats: usize = std::env::var(REPEATS_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_REPEATS);

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

    let documents: usize = chosen.iter().map(|c| c.documents.len()).sum();
    let labelled: usize = chosen.iter().map(|c| c.truth.rows.len()).sum();
    println!(
        "plan: {} cases · {documents} documents x {repeats} runs = {} requests · \
         {labelled} labelled rows · model {} · sampling {}",
        chosen.len(),
        documents * repeats,
        extractor.name(),
        sampling.describe(),
    );
    for case in &chosen {
        println!(
            "  {:<18} {:>3} rows  {} documents",
            case.label,
            case.truth.rows.len(),
            case.documents.len()
        );
    }

    // Planned before the refusal below, so an unconfigured run still reports
    // what both arms would have sent.
    let arithmetic = plan_arithmetic();

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
        let labels: Vec<Decimal> = case.truth.rows.iter().map(|r| r.amount).collect();
        for (document, path) in &case.documents {
            let mut runs = Vec::new();
            for _ in 0..repeats {
                runs.push(score_one(extractor, &labels, path).await);
            }
            rows.push(summarise(&case.label, document, labels.len(), runs));
        }
    }
    report(&rows, extractor.name(), repeats, sampling);

    let mut checked = Vec::new();
    for (pages, hint) in &arithmetic {
        let mut runs = Vec::new();
        for _ in 0..repeats {
            runs.push(check_one(extractor, pages, *hint).await);
        }
        checked.push(summarise_arithmetic(
            tag(&file_name(&pages[0])),
            hint_name(*hint),
            pages.len(),
            runs,
        ));
    }
    report_arithmetic(&checked, repeats);
}

// --- Arithmetic arm ---------------------------------------------------------
//
// Receipts and paystubs have no CSV twin, so there are no labels. Two weaker
// oracles stand in; what each can and cannot prove is in `MODEL_BENCH.md` Part 6.

/// Born-digital documents whose figures can be read back out of a PDF text
/// layer. Named rather than discovered, for the reason [`ABSENT_ENV`] gives.
const ARITHMETIC_ENV: &str = "OMNI_BENCH_ARITHMETIC";

/// Photographed documents. No text layer, so self-consistency is all they carry.
const PHOTOS_ENV: &str = "OMNI_BENCH_PHOTOS";

const ARITHMETIC_SAMPLE_ENV: &str = "OMNI_BENCH_ARITH_SAMPLE";
const DEFAULT_ARITHMETIC_SAMPLE: usize = 6;

/// Extensions the arithmetic arm will send. Kept in step with what
/// `media::format_for` can prepare, plus the PDF path that rasterizes.
const DOCUMENT_EXTS: [&str; 5] = ["pdf", "jpg", "jpeg", "png", "webp"];

/// Filename marker for one document photographed across several images:
/// `<base>-pg-<n>`. These are pages of one document and go in one request.
const MULTI_PAGE_MARKER: &str = "-pg-";

/// The page number in a `-pg-<n>` filename, for ordering within a document.
///
/// Parsed rather than sorted as text for the reason `run_pdftoppm` gives about
/// poppler's own output: `pg-10` sorts before `pg-2`, which would hand the
/// model a document with its pages shuffled.
fn page_number(path: &Path) -> u32 {
    file_name(path)
        .to_ascii_lowercase()
        .rsplit_once(MULTI_PAGE_MARKER)
        .and_then(|(_, tail)| tail.split('.').next().and_then(|n| n.parse().ok()))
        .unwrap_or(0)
}

/// Group files into documents. `<base>-pg-<n>` files become one multi-page
/// document in page order; every other file stands alone.
fn group_pages(files: Vec<PathBuf>) -> Vec<Vec<PathBuf>> {
    let mut groups: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for path in files {
        let name = file_name(&path).to_ascii_lowercase();
        let key = match name.split_once(MULTI_PAGE_MARKER) {
            Some((base, _)) => base.to_string(),
            None => name,
        };
        groups.entry(key).or_default().push(path);
    }
    groups
        .into_values()
        .map(|mut pages| {
            pages.sort_by_key(|p| page_number(p));
            pages
        })
        .collect()
}

/// One document scored with no label set behind it.
/// One call in the arithmetic arm.
struct ArithRun {
    postings: usize,
    /// Whether the model returned the reference total its hint asks for.
    total: bool,
    /// `verify` raised no line-item-sum complaint.
    arithmetic_ok: bool,
    /// Returned figures appearing nowhere in the source text. `None` when the
    /// document has no text layer to check against.
    ungrounded: Option<usize>,
    /// What the confirm-draft screen would do with this extraction.
    review: bool,
    latency: Duration,
    error: Option<String>,
}

/// One document across every run of it.
struct Checked {
    document: String,
    hint: &'static str,
    /// Files this document arrived as. More than one exercises the multi-part
    /// request path, which is the half a single-image signature could not reach.
    pages: usize,
    runs: usize,
    errors: usize,
    /// Summed over the runs that answered.
    postings: usize,
    /// Runs that returned a total, that were arithmetically sound, and that the
    /// confirm screen would flag. Counts rather than booleans: the 2-invented-vs-3
    /// margin this arm was ranked on was already called noise, and a boolean
    /// cannot say whether a run-to-run difference is what produced it.
    totals: usize,
    sound: usize,
    flagged: usize,
    ungrounded: Option<usize>,
    agreed: bool,
    latency: Duration,
    first_error: Option<String>,
}

impl Checked {
    fn answered(&self) -> usize {
        self.runs.saturating_sub(self.errors)
    }
}

/// Pool one document's arithmetic runs into the row that gets printed.
fn summarise_arithmetic(
    document: String,
    hint: &'static str,
    pages: usize,
    runs: Vec<ArithRun>,
) -> Checked {
    let mut c = Checked {
        document,
        hint,
        pages,
        runs: runs.len(),
        errors: 0,
        postings: 0,
        totals: 0,
        sound: 0,
        flagged: 0,
        ungrounded: None,
        agreed: true,
        latency: Duration::ZERO,
        first_error: None,
    };
    let mut latencies = Vec::new();
    let mut shapes = Vec::new();

    for run in &runs {
        if let Some(e) = &run.error {
            c.errors += 1;
            if c.first_error.is_none() {
                c.first_error = Some(e.clone());
            }
            continue;
        }
        latencies.push(run.latency);
        c.postings += run.postings;
        c.totals += usize::from(run.total);
        c.sound += usize::from(run.arithmetic_ok);
        c.flagged += usize::from(run.review);
        // `None` means no text layer, which is a property of the document rather
        // than of the run, so summing only ever adds numbers to numbers.
        if let Some(n) = run.ungrounded {
            c.ungrounded = Some(c.ungrounded.unwrap_or(0) + n);
        }
        shapes.push((
            run.postings,
            run.total,
            run.arithmetic_ok,
            run.ungrounded,
            run.review,
        ));
    }

    c.agreed = shapes.windows(2).all(|w| w[0] == w[1]);
    latencies.sort_unstable();
    c.latency = latencies
        .get(latencies.len() / 2)
        .copied()
        .unwrap_or(Duration::ZERO);
    c
}

fn hint_name(hint: ExtractionHint) -> &'static str {
    match hint {
        ExtractionHint::Receipt => "receipt",
        ExtractionHint::Paystub => "paystub",
        ExtractionHint::BankStatement => "statement",
        ExtractionHint::BrokerageStatement => "brokerage",
        ExtractionHint::EmailBody => "email",
        ExtractionHint::Generic => "generic",
    }
}

fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    }
}

/// Absolute values of every number in `text`.
///
/// Over-collects on purpose: years, hour counts and employee numbers land in
/// the set alongside money. That makes the grounding check a floor on
/// fabrication rather than a ceiling, and a floor is the safe direction — a
/// figure wrongly called invented would need the real document to disprove.
pub(crate) fn figures_in(text: &str) -> BTreeSet<Decimal> {
    let mut found = BTreeSet::new();
    let mut token = String::new();
    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() || ch == ',' || ch == '.' {
            token.push(ch);
            continue;
        }
        if !token.is_empty() {
            let cleaned: String = token.chars().filter(|c| *c != ',').collect();
            if let Ok(value) = Decimal::from_str(cleaned.trim_matches('.')) {
                found.insert(value.abs());
            }
            token.clear();
        }
    }
    found
}

/// The document's own text layer, via poppler. `None` for a photograph or a
/// scan, which is the signal that the grounding oracle does not apply.
fn source_text(path: &Path) -> Option<String> {
    if mime_for(path) != "application/pdf" {
        return None;
    }
    let out = std::process::Command::new("pdftotext")
        .arg("-layout")
        .arg(path)
        .arg("-")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    out.status
        .success()
        .then_some(text)
        .filter(|t| t.chars().any(|c| c.is_ascii_digit()))
}

/// Did `verify` complain that the line items do not add up to the total?
///
/// A substring of another module's warning text, which is a coupling rather
/// than an interface. `the_arithmetic_predicate_still_matches_verifys_wording`
/// exists to make a reword fail loudly instead of reporting every document as
/// arithmetically sound forever.
const SUM_MISMATCH: &str = "does not match document total";

async fn check_one(
    extractor: &dyn DocumentExtractor,
    pages: &[PathBuf],
    hint: ExtractionHint,
) -> ArithRun {
    let mut checked = ArithRun {
        postings: 0,
        total: false,
        arithmetic_ok: false,
        ungrounded: None,
        review: true,
        latency: Duration::ZERO,
        error: None,
    };

    let mut bytes = Vec::with_capacity(pages.len());
    for page in pages {
        match std::fs::read(page) {
            Ok(body) => bytes.push(body),
            Err(e) => {
                checked.error = Some(e.to_string());
                return checked;
            }
        }
    }
    // Read before the request, so a poppler failure is not attributed to the
    // model. Every page's text, because a figure stated on page two is stated.
    let text: String = pages.iter().filter_map(|p| source_text(p)).collect();
    let grounding = (!text.is_empty()).then(|| figures_in(&text));

    let parts: Vec<DocumentPart<'_>> = bytes
        .iter()
        .zip(pages)
        .map(|(body, path)| DocumentPart::new(body, mime_for(path)))
        .collect();

    let started = Instant::now();
    let result = extractor.extract(&parts, hint).await;
    checked.latency = started.elapsed();

    match result {
        Ok(extraction) => {
            let report = verify(&extraction, hint, DEFAULT_CONFIDENCE_THRESHOLD);
            checked.postings = extraction.postings.len();
            checked.total = extraction.total.is_some();
            checked.arithmetic_ok = !report.warnings.iter().any(|w| w.contains(SUM_MISMATCH));
            checked.review = report.needs_manual_review;
            checked.ungrounded = grounding.map(|figures| {
                let postings = extraction
                    .postings
                    .iter()
                    .filter(|p| !figures.contains(&p.amount.abs()))
                    .count();
                let total = extraction
                    .total
                    .is_some_and(|t| !figures.contains(&t.abs()));
                postings + usize::from(total)
            });
        }
        Err(e) => checked.error = Some(e.to_string()),
    }
    checked
}

/// The documents the arithmetic arm will score, and the hint each is read under.
///
/// Two directories rather than one because the hint is a claim about what the
/// document *is*, which an extension cannot supply: a photographed paystub is
/// a JPEG and a born-digital receipt is a PDF.
fn plan_arithmetic() -> Vec<(Vec<PathBuf>, ExtractionHint)> {
    let sample: usize = std::env::var(ARITHMETIC_SAMPLE_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_ARITHMETIC_SAMPLE);

    let mut planned = Vec::new();
    for (env, hint) in [
        (ARITHMETIC_ENV, ExtractionHint::Paystub),
        (PHOTOS_ENV, ExtractionHint::Receipt),
    ] {
        let name = hint_name(hint);
        let Ok(dir) = std::env::var(env) else {
            println!("{name} arm SKIPPED — set {env} to a directory of such documents.");
            continue;
        };
        let mut found = Vec::new();
        collect_documents(Path::new(&dir), &DOCUMENT_EXTS, 0, &mut found);
        found.sort();
        let documents = group_pages(found);
        if documents.is_empty() {
            println!("{name} arm SKIPPED — {env} names a directory with no readable documents.");
            continue;
        }
        // Strided for the same reason the statement arm is: the first N of a
        // corpus filed by date is one period and one layout.
        let stride = (documents.len() / sample.max(1)).max(1);
        let taken: Vec<Vec<PathBuf>> = documents
            .iter()
            .step_by(stride)
            .take(sample)
            .cloned()
            .collect();
        let grounded = taken
            .iter()
            .filter(|pages| pages.iter().any(|p| source_text(p).is_some()))
            .count();
        let multi = taken.iter().filter(|pages| pages.len() > 1).count();
        println!(
            "{name} arm: {} of {} documents · {grounded} carry a text layer to check against \
             · {multi} span several files",
            taken.len(),
            documents.len(),
        );
        planned.extend(taken.into_iter().map(|pages| (pages, hint)));
    }
    planned
}

fn report_arithmetic(rows: &[Checked], repeats: usize) {
    if rows.is_empty() {
        return;
    }
    println!();
    println!(
        "{:<6} {:<10} {:>5} {:>5} {:>6} {:>7} {:>7} {:>7} {:>4} {:>6} {:>8}",
        "DOC",
        "HINT",
        "PAGES",
        "POST",
        "TOTAL",
        "SOUND",
        "UNGRND",
        "FLAG",
        "ERR",
        "AGREE",
        "MEDIAN"
    );
    for row in rows {
        let answered = row.answered();
        let agree = match (answered, row.agreed) {
            (0 | 1, _) => "n/a",
            (_, true) => "yes",
            (_, false) => "NO",
        };
        let out_of = |n: usize| {
            if answered == 0 {
                "—".to_string()
            } else {
                format!("{n}/{answered}")
            }
        };
        println!(
            "{:<6} {:<10} {:>5} {:>5} {:>6} {:>7} {:>7} {:>7} {:>4} {:>6} {:>7.1}s  {}",
            row.document,
            row.hint,
            row.pages,
            mean_per_run(row.postings, answered),
            out_of(row.totals),
            out_of(row.sound),
            row.ungrounded
                .map_or_else(|| "n/a".to_string(), |n| n.to_string()),
            out_of(row.flagged),
            row.errors,
            agree,
            row.latency.as_secs_f64(),
            row.first_error.as_deref().unwrap_or(""),
        );
    }
    println!(
        "  POST is per run; TOTAL, SOUND and FLAG are runs out of those that \
         answered; UNGRND is summed over {repeats} runs."
    );

    let scored: Vec<&Checked> = rows.iter().filter(|r| r.answered() > 0).collect();
    if scored.is_empty() {
        println!("every document errored — nothing was measured");
        return;
    }
    // Every run, not any: a document that was sound twice and wrong once is not a
    // document this seat read soundly, and rounding it up is how a margin of two
    // fabrications becomes a ranking.
    let sound = scored.iter().filter(|r| r.sound == r.answered()).count();
    let with_total = scored.iter().filter(|r| r.totals == r.answered()).count();
    let flagged = scored.iter().filter(|r| r.flagged > 0).count();
    let unstable = scored.iter().filter(|r| !r.agreed).count();
    println!(
        "{} documents: {sound} arithmetically sound on every run · {with_total} returned a \
         total on every run · {flagged} would be flagged at least once · {unstable} answered \
         differently between runs",
        scored.len()
    );
    // Reported apart, because it is the only column with ground truth behind it.
    let checkable: Vec<&&Checked> = scored.iter().filter(|r| r.ungrounded.is_some()).collect();
    if !checkable.is_empty() {
        let clean = checkable.iter().filter(|r| r.ungrounded == Some(0)).count();
        let invented: usize = checkable.iter().filter_map(|r| r.ungrounded).sum();
        println!(
            "{} with a text layer: {clean} used only figures the document states, \
             {invented} figures appear nowhere in it",
            checkable.len()
        );
    }
    let errored_runs: usize = rows.iter().map(|r| r.errors).sum();
    println!(
        "{} documents errored on every run · {errored_runs} errored runs in total",
        rows.len() - scored.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use omni_me_core::extraction::{ExtractedPosting, ExtractionResult};

    fn dec(v: &str) -> Decimal {
        Decimal::from_str(v).unwrap()
    }

    fn run_of(matched: usize, sign_flipped: usize, fabricated: usize) -> Run {
        Run {
            returned: matched + fabricated,
            matched,
            sign_flipped,
            fabricated,
            latency: Duration::from_millis(100),
            error: None,
        }
    }

    /// The whole reason this arm gained repeats: 13 sign-flips on one run of a
    /// statement and 0 on the next, from the same model on the same work.
    #[test]
    fn a_document_scored_differently_between_runs_is_flagged() {
        let row = summarise(
            "case",
            "doc",
            3,
            vec![run_of(3, 0, 0), run_of(3, 0, 0), run_of(1, 2, 0)],
        );
        assert!(!row.agreed);
        assert_eq!(row.sign_flipped, 2, "summed, not averaged away");
        // Pooled against three runs of three labels each, so 7 of 9.
        assert_eq!(row.matched, 7);
        assert!((row.recall() - 7.0 / 9.0).abs() < 1e-9);
    }

    #[test]
    fn identical_runs_agree_and_recall_is_unchanged_by_repeating_them() {
        let row = summarise(
            "case",
            "doc",
            4,
            vec![run_of(4, 0, 0), run_of(4, 0, 0), run_of(4, 0, 0)],
        );
        assert!(row.agreed);
        assert!((row.recall() - 1.0).abs() < f64::EPSILON);
    }

    /// An errored run is not a score of zero — that would report an endpoint
    /// outage as a model that recalled nothing.
    #[test]
    fn an_errored_run_is_excluded_from_the_score_and_the_median() {
        let mut errored = run_of(0, 0, 0);
        errored.error = Some("upstream".into());
        errored.latency = Duration::from_millis(5);
        let row = summarise("case", "doc", 2, vec![run_of(2, 0, 0), errored]);

        assert_eq!(row.errors, 1);
        assert_eq!(row.answered(), 1);
        assert!((row.recall() - 1.0).abs() < f64::EPSILON);
        assert_eq!(row.latency, Duration::from_millis(100));
        assert_eq!(row.first_error.as_deref(), Some("upstream"));
        // One answer cannot disagree with anything, and the report prints n/a.
        assert!(row.agreed);
    }

    #[test]
    fn a_document_every_run_refused_reports_no_recall_rather_than_zero() {
        let errored = || {
            let mut r = run_of(0, 0, 0);
            r.error = Some("429".into());
            r
        };
        let row = summarise("case", "doc", 5, vec![errored(), errored()]);
        assert_eq!(row.answered(), 0);
        // `report` filters these out before the aggregate; recall must not drag
        // the mean toward zero if one ever reaches it.
        assert!((row.recall() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_pooled_count_prints_per_run_and_only_decimalises_when_runs_differ() {
        assert_eq!(mean_per_run(9, 3), "3");
        assert_eq!(mean_per_run(10, 3), "3.3");
        assert_eq!(mean_per_run(4, 0), "—");
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

    fn extraction(postings: &[&str], total: Option<&str>) -> ExtractionResult {
        ExtractionResult {
            date: None,
            date_as_printed: None,
            description: None,
            postings: postings
                .iter()
                .map(|amount| ExtractedPosting {
                    account_hint: None,
                    commodity: "CAD".into(),
                    amount: dec(amount),
                    line_label: None,
                })
                .collect(),
            total: total.map(dec),
            confidence: 0.9,
            model: "test".into(),
            dropped_postings: 0,
            total_as_printed: None,
            total_discarded: false,
            document_kind: None,
            order_ref: None,
            raw_response: serde_json::Value::Null,
        }
    }

    #[test]
    fn the_arithmetic_predicate_still_matches_verifys_wording() {
        // SUM_MISMATCH is a substring of another module's message rather than an
        // interface. This test is that coupling made loud: reword the warning and
        // it fails here, instead of the arm reporting every document as sound.
        let wrong = extraction(&["10.00", "5.00"], Some("99.00"));
        let complained = verify(
            &wrong,
            ExtractionHint::Receipt,
            DEFAULT_CONFIDENCE_THRESHOLD,
        );
        assert!(
            complained.warnings.iter().any(|w| w.contains(SUM_MISMATCH)),
            "verify no longer says {SUM_MISMATCH:?}: {:?}",
            complained.warnings
        );

        let right = extraction(&["10.00", "5.00"], Some("15.00"));
        let quiet = verify(
            &right,
            ExtractionHint::Receipt,
            DEFAULT_CONFIDENCE_THRESHOLD,
        );
        assert!(!quiet.warnings.iter().any(|w| w.contains(SUM_MISMATCH)));
    }

    #[test]
    fn figures_are_read_with_separators_stripped_and_signs_discarded() {
        let found = figures_in("Gross    1,234.56\nTax        (98.70)\nYear 2025");
        assert!(
            found.contains(&dec("1234.56")),
            "a thousands separator is not part of the figure"
        );
        assert!(
            found.contains(&dec("98.70")),
            "a parenthesised deduction states the same figure"
        );
        assert!(found.contains(&dec("2025")));
        assert!(!found.contains(&dec("1.23")));
    }

    #[test]
    fn a_figure_the_document_never_states_counts_as_ungrounded() {
        let stated = figures_in("Net pay 2,410.88");
        assert!(stated.contains(&dec("-2410.88").abs()));
        assert!(
            !stated.contains(&dec("2410.89")),
            "a one-cent misread has no twin in the text and must not pass as grounded"
        );
    }

    #[test]
    fn pages_of_one_document_group_into_one_case() {
        // Half a receipt scored against a total printed on the other half is an
        // arithmetic mismatch caused by the harness, so the pages go in one
        // request. Standalone files are untouched by the grouping.
        let grouped = group_pages(vec![
            PathBuf::from("x/receipt-1-pg-1.jpg"),
            PathBuf::from("x/receipt-3.jpg"),
            PathBuf::from("x/receipt-1-pg-2.jpg"),
        ]);
        assert_eq!(grouped.len(), 2);
        let multi = grouped.iter().find(|g| g.len() == 2).expect("one group");
        assert_eq!(file_name(&multi[0]), "receipt-1-pg-1.jpg", "page order");
        assert_eq!(file_name(&multi[1]), "receipt-1-pg-2.jpg");
    }

    #[test]
    fn page_ten_does_not_sort_before_page_two() {
        // The same trap `run_pdftoppm` documents in poppler's output. Sorting
        // these as text hands the model a shuffled document.
        let grouped = group_pages(vec![
            PathBuf::from("x/scan-pg-10.jpg"),
            PathBuf::from("x/scan-pg-2.jpg"),
        ]);
        assert_eq!(grouped.len(), 1);
        assert_eq!(file_name(&grouped[0][0]), "scan-pg-2.jpg");
        assert_eq!(file_name(&grouped[0][1]), "scan-pg-10.jpg");
    }

    #[test]
    fn the_mime_follows_the_extension_and_defaults_to_a_photo() {
        assert_eq!(mime_for(Path::new("a/b.PDF")), "application/pdf");
        assert_eq!(mime_for(Path::new("a/b.png")), "image/png");
        assert_eq!(mime_for(Path::new("a/receipt-1.jpg")), "image/jpeg");
        // An unknown extension reads as a photo rather than being refused: the
        // endpoint decides what it accepts, and this arm's corpus is photographs.
        assert_eq!(mime_for(Path::new("a/scan")), "image/jpeg");
    }
}
