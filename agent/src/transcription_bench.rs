//! `--bench-transcription`: role C3, scored against the text layer it cannot see.
//!
//! Test scaffolding, on the same terms as [`super::ask`].
//!
//! What the numbers mean and what this instrument cannot see: `MODEL_BENCH.md`
//! Part 9.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use omni_me_core::extraction::media;
use omni_me_core::extraction::transcribe::DocumentTranscriber;
use omni_me_core::extraction::{DocumentPart, MAX_DOCUMENT_PARTS};
use omni_me_core::statement::pdf;
use rust_decimal::Decimal;

use crate::extraction_bench::figures_in;

/// Where the corpus lives. Not in the repo and never will be.
const CORPUS_ENV: &str = "OMNI_BENCH_CORPUS";
const DEFAULT_CORPUS: &str = ".reference/paisa-ledger";

/// How many documents one run scores.
///
/// Small by default: every case is a whole document rendered to images, which is
/// the most expensive request shape this project sends.
const SAMPLE_ENV: &str = "OMNI_BENCH_TRANSCRIBE_SAMPLE";
const DEFAULT_SAMPLE: usize = 6;

/// A born-digital PDF: one whose own text layer is the answer key.
struct Case {
    path: PathBuf,
    /// What the file states, via poppler. The oracle, free and exact.
    truth: String,
}

/// Every PDF under `corpus`, depth-bounded, in a stable order.
///
/// ⚠️ Sorted, and not cosmetically: directory order is filesystem order, so two
/// runs would otherwise sample different documents and their numbers would not
/// be comparable. The account directories are discovered rather than listed,
/// because they are named after the institutions that issued them and this file
/// is public.
fn collect_pdfs(dir: &Path, depth: usize, into: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_pdfs(&path, depth - 1, into);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
        {
            into.push(path);
        }
    }
}

/// Keep only the PDFs that carry a real text layer.
///
/// ⛔ This filter *is* the oracle. A scanned PDF has no text layer, so there
/// would be nothing to score against — and scoring a transcription against an
/// empty string would report every model as perfectly fabricating.
async fn born_digital(paths: Vec<PathBuf>, want: usize) -> (Vec<Case>, usize, usize) {
    let mut cases = Vec::new();
    let mut scanned = 0usize;
    let mut unreadable = 0usize;
    for path in paths {
        if cases.len() >= want {
            break;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            unreadable += 1;
            continue;
        };
        match pdf::extract_layout_text(&bytes, "").await {
            Ok(text) if text.trim().len() > 200 => cases.push(Case { path, truth: text }),
            Ok(_) => scanned += 1,
            Err(_) => unreadable += 1,
        }
    }
    (cases, scanned, unreadable)
}

/// Words, lowercased, with surrounding punctuation trimmed.
///
/// A multiset would be stricter, but `-layout` output repeats column headers in
/// ways a transcription reasonably does not, and counting those as misses would
/// measure poppler's spacing rather than the model's reading.
fn words(text: &str) -> BTreeSet<String> {
    text.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| w.len() > 1)
        .collect()
}

struct Scored {
    tag: String,
    pages: usize,
    /// Words the file states that came back.
    words_found: usize,
    words_total: usize,
    /// Figures the file states that came back. Tracked apart from words because
    /// this project's documents are mostly money, and a dropped digit matters in
    /// a way a dropped article does not.
    figures_found: usize,
    figures_total: usize,
    /// Figures the transcription states that the file does not. The fabrication
    /// column, and the one that ranks first.
    figures_invented: usize,
    latency: Duration,
    error: Option<String>,
    /// A few invented figures, so the number names something to go and look at.
    sample: Vec<String>,
}

fn score(case: &Case, transcribed: &str, pages: usize, latency: Duration) -> Scored {
    let truth_words = words(&case.truth);
    let got_words = words(transcribed);
    let truth_figs = figures_in(&case.truth);
    let got_figs = figures_in(transcribed);

    let invented: Vec<&Decimal> = got_figs.difference(&truth_figs).collect();
    Scored {
        tag: tag_for(&case.path),
        pages,
        words_found: truth_words.intersection(&got_words).count(),
        words_total: truth_words.len(),
        figures_found: truth_figs.intersection(&got_figs).count(),
        figures_total: truth_figs.len(),
        figures_invented: invented.len(),
        latency,
        error: None,
        sample: invented.iter().take(4).map(|d| d.to_string()).collect(),
    }
}

/// A stable, non-identifying label for a document.
///
/// FNV-1a over the file stem, four hex digits — the same construction
/// `extraction_bench::tag` uses, so the two scorecards are read the same way.
/// Deterministic, so two runs of one corpus stay comparable.
///
/// Dropping the directory is not enough on its own. An earlier version of this
/// kept the digits of the file name, which are the statement's own date: it
/// printed `doc-20190630`, a real date out of the user's financial records,
/// into output whose whole purpose is to be pasted somewhere public.
fn tag_for(path: &Path) -> String {
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in name.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("doc-{:04x}", hash & 0xffff)
}

async fn run_one(transcriber: &dyn DocumentTranscriber, case: &Case) -> Scored {
    let bytes = match std::fs::read(&case.path) {
        Ok(b) => b,
        Err(e) => return failed(case, 0, format!("read: {e}")),
    };

    // ⛔ Rasterized here rather than handed over as a PDF, and that is the whole
    // trick. `payload_for` sends a PDF's text layer when it has one, so passing
    // the file would measure poppler reading itself. Rendering to images forces
    // the vision path onto a document whose answer is already known.
    let pages = match media::rasterize_pdf(&bytes).await {
        Ok(p) => p,
        Err(e) => return failed(case, 0, format!("rasterize: {e}")),
    };
    if pages.len() > MAX_DOCUMENT_PARTS {
        return failed(
            case,
            pages.len(),
            "more pages than one request carries".into(),
        );
    }

    let parts: Vec<DocumentPart<'_>> = pages
        .iter()
        .map(|p| DocumentPart::new(&p.bytes, p.mime))
        .collect();

    let started = Instant::now();
    let out = transcriber.transcribe(&parts).await;
    let latency = started.elapsed();
    match out {
        Ok(text) => score(case, &text, pages.len(), latency),
        Err(e) => failed(case, pages.len(), e.to_string()),
    }
}

fn failed(case: &Case, pages: usize, error: String) -> Scored {
    Scored {
        tag: tag_for(&case.path),
        pages,
        words_found: 0,
        words_total: 0,
        figures_found: 0,
        figures_total: 0,
        figures_invented: 0,
        latency: Duration::ZERO,
        error: Some(error),
        sample: Vec::new(),
    }
}

fn report(rows: &[Scored], model: &str) {
    println!("\nrole C3 — transcription · model {model}\n");
    println!(
        "{:<12} {:>6} {:>12} {:>12} {:>8} {:>9}",
        "doc", "PAGES", "WORDS", "FIGURES", "INVENT", "LATENCY"
    );
    for r in rows {
        if let Some(e) = &r.error {
            println!("{:<12} {:>6} {:>12}  {e}", r.tag, r.pages, "ERROR");
            continue;
        }
        println!(
            "{:<12} {:>6} {:>5}/{:<6} {:>5}/{:<6} {:>8} {:>7}ms",
            r.tag,
            r.pages,
            r.words_found,
            r.words_total,
            r.figures_found,
            r.figures_total,
            r.figures_invented,
            r.latency.as_millis(),
        );
    }

    let scored: Vec<&Scored> = rows.iter().filter(|r| r.error.is_none()).collect();
    let invented: usize = scored.iter().map(|r| r.figures_invented).sum();
    let figs_found: usize = scored.iter().map(|r| r.figures_found).sum();
    let figs_total: usize = scored.iter().map(|r| r.figures_total).sum();
    let words_found: usize = scored.iter().map(|r| r.words_found).sum();
    let words_total: usize = scored.iter().map(|r| r.words_total).sum();
    let errors = rows.len() - scored.len();

    println!(
        "\nranking keys, in order: invented figures ({invented}) · figure recall ({figs_found}/{figs_total}) \
         · word recall ({words_found}/{words_total})"
    );
    if errors > 0 {
        println!(
            "  ⚠ {errors} document(s) errored and are scored by nothing. Read this before the \
             ranking."
        );
    }
    for r in scored.iter().filter(|r| !r.sample.is_empty()) {
        println!("  {} invented figures: {}", r.tag, r.sample.join(", "));
    }
}

pub async fn run(transcriber: Option<&dyn DocumentTranscriber>) {
    let corpus =
        PathBuf::from(std::env::var(CORPUS_ENV).unwrap_or_else(|_| DEFAULT_CORPUS.to_string()));
    let want: usize = std::env::var(SAMPLE_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_SAMPLE);

    let mut pdfs = Vec::new();
    collect_pdfs(&corpus, 5, &mut pdfs);
    if pdfs.is_empty() {
        eprintln!(
            "no PDFs under {} — point {CORPUS_ENV} at the corpus. Refusing rather than \
             reporting an empty sweep as a scorecard.",
            corpus.display()
        );
        return;
    }

    let (cases, scanned, unreadable) = born_digital(pdfs, want).await;
    println!(
        "plan: {} born-digital document(s) of {want} wanted · {scanned} skipped as scans \
         (no text layer to score against) · {unreadable} unreadable",
        cases.len()
    );
    println!(
        "oracle: render each PDF to images, transcribe the images, compare against the text \
         layer the file already carries"
    );
    if cases.is_empty() {
        eprintln!("\nno born-digital PDFs found — nothing here has an answer key.");
        return;
    }

    // No transcriber is `None` rather than a null object, so nothing can be
    // mistaken for a model that answered badly — but refuse anyway, so the plan
    // above stays usable as a zero-token dry run.
    let Some(transcriber) = transcriber else {
        eprintln!(
            "\nno [llm.extractor] openai_compatible vision endpoint — set one. Refusing \
             rather than reporting a missing endpoint as a scorecard."
        );
        return;
    };

    let mut rows = Vec::new();
    for case in &cases {
        rows.push(run_one(transcriber, case).await);
    }
    report(&rows, transcriber.name());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tag_hides_the_statement_date_it_is_built_from() {
        let first = tag_for(Path::new(
            "/corpus/globepay_chequing/2019-06/2019-06-30.pdf",
        ));
        assert_eq!(
            first,
            tag_for(Path::new(
                "/corpus/globepay_chequing/2019-06/2019-06-30.pdf"
            )),
            "must be stable across runs or two scorecards cannot be compared"
        );
        assert_ne!(
            first,
            tag_for(Path::new(
                "/corpus/globepay_chequing/2026-04/2026-04-30.pdf"
            ))
        );
        // The regression this guards: a tag built from the file name's digits
        // printed the statement's own date into public output.
        assert!(!first.contains("2019"));
        assert!(!first.contains("0630"));
        assert!(!first.contains("globepay"));
        assert_eq!(first.len(), "doc-".len() + 4);

        // The directory half of the same claim. The test this replaces checked
        // only this, and asserted the digits survived — so the institution name
        // was guarded while the statement date sat next to it, unexamined.
        let t = tag_for(Path::new(
            "/home/x/BigBank Personal/2024/03/statement-8821.pdf",
        ));
        assert!(!t.contains("BigBank"));
        assert!(!t.contains("8821"));
    }

    #[test]
    fn words_ignores_punctuation_and_case_but_not_digits() {
        let got = words("Invoice, NW-88213: total 142.65");
        assert!(got.contains("invoice"));
        assert!(got.contains("nw-88213"), "got {got:?}");
        assert!(got.contains("142.65"));
    }

    #[test]
    fn a_perfect_transcription_scores_full_recall_and_no_fabrication() {
        let case = Case {
            path: PathBuf::from("stmt-001.pdf"),
            truth: "Opening balance 1,200.00\nCoffee 4.50\nClosing 1,195.50".into(),
        };
        let s = score(&case, &case.truth, 1, Duration::ZERO);
        assert_eq!(s.figures_found, s.figures_total);
        assert_eq!(s.figures_invented, 0);
        assert_eq!(s.words_found, s.words_total);
    }

    #[test]
    fn a_misread_figure_shows_as_both_a_miss_and_a_fabrication() {
        // The failure this instrument exists for: 1,195.50 read as 1,196.50 is
        // one figure not found AND one figure invented, and both halves matter.
        let case = Case {
            path: PathBuf::from("stmt-002.pdf"),
            truth: "Closing 1,195.50".into(),
        };
        let s = score(&case, "Closing 1,196.50", 1, Duration::ZERO);
        assert_eq!(s.figures_found, 0, "the real figure did not come back");
        assert_eq!(s.figures_invented, 1);
        assert_eq!(s.sample, vec!["1196.50".to_string()]);
    }

    #[test]
    fn an_empty_transcription_is_zero_recall_not_an_error() {
        // A model that answers "" on a page full of text has abstained wrongly,
        // and that must show as a recall failure rather than vanish.
        let case = Case {
            path: PathBuf::from("stmt-003.pdf"),
            truth: "Closing 1,195.50".into(),
        };
        let s = score(&case, "", 1, Duration::ZERO);
        assert_eq!(s.figures_found, 0);
        assert_eq!(s.figures_invented, 0);
        assert!(s.error.is_none());
    }
}
