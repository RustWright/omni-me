//! `--bench-retrieval`: is meaning-based retrieval actually better, and what does
//! each model cost to run?
//!
//! ⚠️ **Test scaffolding**, on the same terms as [`super::ask`].
//!
//! ## Why this is not part of `--bench`
//!
//! [`super::bench`] scores an LLM's verb selection: it needs credentials, an
//! endpoint, a rate limit, and it costs tokens. Retrieval needs none of those — it
//! is local, deterministic, and repeatable. Folding the two together would make the
//! retrieval number cost money to obtain and would tie a local regression check to
//! a vendor being up.
//!
//! ## Why the corpus is a fixture rather than the real journal
//!
//! Two reasons, and the second is the one that matters. The obvious one is that a
//! measurement anyone can re-run cannot depend on private data. The subtler one is
//! that a labelled question set *is* the measurement: "the right answer to this
//! question is that record" has to be written down by hand, and writing it down over
//! real entries would still be a fixture — just an unshareable one.
//!
//! ⚠️ **The honest limitation: the same hand wrote the corpus and the questions.**
//! A fixture built only from paraphrase cases would flatter embeddings by
//! construction, since keyword search cannot win a case with no shared words. So the
//! cases are labelled [`CaseKind::Lexical`] or [`CaseKind::Semantic`] and both are
//! reported separately. A change that improves the semantic column while wrecking
//! the lexical one is a regression, and the split column is what makes that visible.
//!
//! ## What is reported
//!
//! Three arms over identical data: keyword only (what shipped before Phase C), fused
//! (BM25 + vectors through RRF), and fused plus each reranker. Per arm: how often the
//! right record came back at all, how often it came back *first*, and mean reciprocal
//! rank. Per model: resident memory and per-query wall clock, because the deployment
//! host has 3820 MB, no swap and two cores — a model that wins on rank and does not
//! fit has not won.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use omni_me_core::assistant::{
    Embedder, Rerank, RerankService, Reranker, Retrievers, SemanticSearch, VectorSearch,
    dispatch_with, vector_store,
};
use omni_me_core::config::{
    ConfigKey, ConfigMap, ConfigValue, Feature, RERANK_MODEL_VALUES, ResolvedConfig,
};
use omni_me_core::db::Database;
use omni_me_core::events::{NotesProjection, Projection};

/// How many results a case asks for. Wider than anyone would read, so the scorecard
/// can distinguish "not found" from "found at rank nine".
const LIMIT: usize = 10;

/// Whether a case's words appear in the record that answers it.
///
/// The distinction is the whole point of the split scorecard: `Lexical` cases are
/// ones BM25 can win and a vector index could plausibly *lose*, so they are the
/// guard against a change that trades one kind of recall for the other.
#[derive(Debug, Clone, Copy, PartialEq)]
enum CaseKind {
    /// The question shares a distinctive term with the answer.
    Lexical,
    /// The question and the answer share no content words at all.
    Semantic,
}

struct Case {
    question: &'static str,
    /// `record_type` and the fixture id of the one record that answers it.
    expect_type: &'static str,
    expect_id: &'static str,
    kind: CaseKind,
}

// ── The fixture corpus ──────────────────────────────────────────────────────
//
// Fictional throughout: no real names, places, institutions or amounts. Written
// as a plausible small personal corpus rather than as retrieval test data, because
// text authored to be retrievable is text that flatters every retriever.

/// `(id, title, body)`.
const NOTES: &[(&str, &str, &str)] = &[
    (
        "n01",
        "Flat renewal",
        "The landlord is putting the monthly figure up by four percent from March. Signed the \
         extension for another year rather than move again.",
    ),
    (
        "n02",
        "Sourdough starter schedule",
        "Feed at eight and again at eight. Equal weights flour and water. It doubles in about \
         five hours when the kitchen is warm.",
    ),
    (
        "n03",
        "Rainy day fund",
        "Three months of outgoings, kept somewhere I cannot reach in an afternoon. Topped up \
         whenever a month comes in under.",
    ),
    (
        "n04",
        "Car insurance renewal",
        "Quote came back higher than last year. Worth calling around before it renews \
         automatically in August.",
    ),
    (
        "n05",
        "Mechanic invoice",
        "Two hundred and eighty for the timing belt and a new set of front pads. They kept it \
         overnight and rang in the morning.",
    ),
    (
        "n06",
        "Vehicle registration",
        "Runs out at the end of the quarter. Needs the roadworthy certificate first, which \
         means booking the inspection.",
    ),
    (
        "n07",
        "Guitar practice plan",
        "Twenty minutes of scales before anything else. Then one song, slowly, until the \
         changes stop being a decision.",
    ),
    (
        "n08",
        "Dentist appointment",
        "Tuesday the fourteenth at ten past four. Bring the referral letter and the old \
         X-rays if I can find them.",
    ),
    (
        "n09",
        "Dentist recommendations",
        "Two names from Priya, one from the noticeboard at the pool. All three take new \
         patients, none of them do evenings.",
    ),
    (
        "n10",
        "Passport renewal",
        "Application, two photographs to the specified size, the old book, and the fee. Allow \
         six weeks and do not book anything before it lands.",
    ),
    (
        "n11",
        "Bike maintenance",
        "Chain wear checked with the gauge, brake pads have a season left, and the rear wheel \
         needs truing before winter.",
    ),
    (
        "n12",
        "Reading list",
        "Two novels, a book about tides, and the one on medieval agriculture that everyone \
         says is better than it sounds.",
    ),
    (
        "n13",
        "Coffee grinder settings",
        "Fourteen for the pour over, eight for the moka pot. It drifts coarser as the burrs \
         wear, so recheck monthly.",
    ),
    (
        "n14",
        "Kitchen tap dripping",
        "Washer, most likely. Isolating valve is under the sink on the left and turns the \
         wrong way from what you expect.",
    ),
    (
        "n15",
        "Wattle Street parking",
        "Two hours free before six, unrestricted after. The signs on the north side say \
         something different from the ones opposite.",
    ),
    (
        "n16",
        "Tax paperwork",
        "Receipts for the home office share, the professional membership, and the two \
         courses. Everything else is already reported.",
    ),
    (
        "n17",
        "Houseplant watering",
        "The fern wants damp and the rest want dry. Nothing gets watered on a schedule; check \
         the top inch first.",
    ),
    (
        "n18",
        "Gift ideas",
        "Something for the garden for Dad. Nothing that needs assembling, nothing that needs \
         charging, nothing with a subscription.",
    ),
    (
        "n19",
        "Running shoes",
        "The current pair are at about six hundred kilometres. The shop said the foam goes \
         long before the tread looks worn.",
    ),
    (
        "n20",
        "Insurance excess",
        "Five hundred on the contents policy, one thousand on the car. Worth reconsidering \
         the first one at renewal.",
    ),
    (
        "n21",
        "Bread flour comparison",
        "The strong white from the mill is noticeably better than the supermarket one, and \
         about the same price if bought in the larger sack.",
    ),
    (
        "n22",
        "Emergency contacts",
        "Priya first, then Dad. The building manager is on the fridge and the after-hours \
         plumber is in the drawer.",
    ),
    (
        "n23",
        "Language app streak",
        "Fifteen minutes most mornings. The streak is doing more work than the lessons at \
         this point, which is probably the wrong way round.",
    ),
    (
        "n24",
        "Camera settings for the market",
        "Wide open, low shutter, and let the noise happen. Everything stopped down came back \
         flat and lifeless.",
    ),
];

/// `(id, date, body)`.
const JOURNAL: &[(&str, &str, &str)] = &[
    (
        "j01",
        "2026-02-03",
        "Lay awake until nearly three with my mind going round the same three things. Got up, \
         read for twenty minutes, went back down eventually.",
    ),
    (
        "j02",
        "2026-02-05",
        "Long day. Nothing went wrong exactly but by four I was reading the same paragraph \
         over and over and could not make myself care about any of it.",
    ),
    (
        "j03",
        "2026-02-07",
        "Met Priya at the place on the corner. Two hours, mostly about her move and the \
         thing with her brother. Walked back the long way.",
    ),
    (
        "j04",
        "2026-02-09",
        "Knee complained on the downhill again, about four kilometres in. Walked the last \
         stretch. It is the same spot as last spring.",
    ),
    (
        "j05",
        "2026-02-11",
        "Cleaned out the hall cupboard, which took the whole morning and produced two bags \
         for the charity shop and one genuinely useful box of cables.",
    ),
    (
        "j06",
        "2026-02-13",
        "Signed the extension. Relieved more than anything, even at the higher figure. \
         Moving would have cost more than the difference.",
    ),
    (
        "j07",
        "2026-02-15",
        "First proper attempt at the second half of the song. The changes are still a \
         decision rather than a reflex but it is closer than last week.",
    ),
    (
        "j08",
        "2026-02-17",
        "Rain all day. Made the bread, read most of the tides book, did not leave the flat, \
         and felt fine about all three.",
    ),
    (
        "j09",
        "2026-02-19",
        "The car went in first thing. They rang at eleven with a longer list than expected \
         and I said yes to all of it because what else do you say.",
    ),
    (
        "j10",
        "2026-02-21",
        "Parked on Wattle Street and came back to find I had misread the sign on that side. \
         No ticket, but only by luck.",
    ),
    (
        "j11",
        "2026-02-23",
        "Good stretch of work in the morning, the kind where the shape of the thing finally \
         shows up. Then two meetings undid most of it.",
    ),
    (
        "j12",
        "2026-02-25",
        "Swam properly for the first time in months. Slow, but the breathing came back faster \
         than I expected it to.",
    ),
    (
        "j13",
        "2026-02-27",
        "Dad rang about the garden. He does not want anything, which is the problem, and he \
         will say so about whatever arrives.",
    ),
    (
        "j14",
        "2026-03-01",
        "Woke at five and could not get back down. Made coffee at half past and watched it \
         get light, which was not the worst outcome.",
    ),
    (
        "j15",
        "2026-03-03",
        "Went back to the market with the camera. Everything shot wide open, most of it \
         unusable, four frames I am genuinely pleased with.",
    ),
    (
        "j16",
        "2026-03-05",
        "Sat down to do the paperwork and got about a third of the way before losing the \
         thread. The receipts are at least in one place now.",
    ),
    (
        "j17",
        "2026-03-07",
        "Priya's last day before the move. The whole group came, which surprised her, and \
         nobody made a speech, which was the right call.",
    ),
    (
        "j18",
        "2026-03-09",
        "The tap has got worse. Put a bowl under it, which is not a fix, and looked up how \
         hard it actually is. Harder than a washer, possibly.",
    ),
    (
        "j19",
        "2026-03-11",
        "Ran the loop without stopping for the first time since autumn. The knee held. Slow, \
         but held.",
    ),
    (
        "j20",
        "2026-03-13",
        "Quiet one. Fed the starter, watered what needed it, did the fifteen minutes, and \
         went to bed early enough to notice.",
    ),
];

/// The labelled questions. Each names exactly one right answer.
///
/// ⚠️ **A case with more than one defensible answer is a bad case**, not a hard one:
/// the scorecard would then punish a retriever for finding the other right record.
/// Where two records were plausible the fixture was changed, not the label.
const CASES: &[Case] = &[
    // ── Semantic: the question and its answer share no content words ──
    Case {
        question: "rent increase",
        expect_type: "note",
        expect_id: "n01",
        kind: CaseKind::Semantic,
    },
    Case {
        question: "trouble getting to sleep",
        expect_type: "journal",
        expect_id: "j01",
        kind: CaseKind::Semantic,
    },
    Case {
        question: "feeling burnt out at work",
        expect_type: "journal",
        expect_id: "j02",
        kind: CaseKind::Semantic,
    },
    Case {
        question: "money set aside for emergencies",
        expect_type: "note",
        expect_id: "n03",
        kind: CaseKind::Semantic,
    },
    Case {
        question: "learning an instrument",
        expect_type: "note",
        expect_id: "n07",
        kind: CaseKind::Semantic,
    },
    Case {
        question: "who did I catch up with",
        expect_type: "journal",
        expect_id: "j03",
        kind: CaseKind::Semantic,
    },
    Case {
        question: "pain in my leg while jogging",
        expect_type: "journal",
        expect_id: "j04",
        kind: CaseKind::Semantic,
    },
    Case {
        question: "how much did the car repair cost",
        expect_type: "note",
        expect_id: "n05",
        kind: CaseKind::Semantic,
    },
    // ── Lexical: BM25 can win these, and a regression here is a real loss ──
    Case {
        question: "sourdough",
        expect_type: "note",
        expect_id: "n02",
        kind: CaseKind::Lexical,
    },
    Case {
        question: "Wattle Street parking restrictions",
        expect_type: "note",
        expect_id: "n15",
        kind: CaseKind::Lexical,
    },
    Case {
        question: "passport renewal",
        expect_type: "note",
        expect_id: "n10",
        kind: CaseKind::Lexical,
    },
    Case {
        question: "coffee grinder settings",
        expect_type: "note",
        expect_id: "n13",
        kind: CaseKind::Lexical,
    },
    // ── Distractor-heavy: several records match the words, one answers ──
    Case {
        question: "when is my dentist appointment",
        expect_type: "note",
        expect_id: "n08",
        kind: CaseKind::Lexical,
    },
    Case {
        // Lexical, and the hardest of them: four records say "car" and only this
        // one is the day it went in. Exactly the shape a reranker should fix.
        question: "the day I got the car back from the garage",
        expect_type: "journal",
        expect_id: "j09",
        kind: CaseKind::Lexical,
    },
    Case {
        question: "how worn are my running shoes",
        expect_type: "note",
        expect_id: "n19",
        kind: CaseKind::Lexical,
    },
    Case {
        question: "the leak in the kitchen is getting worse",
        expect_type: "journal",
        expect_id: "j18",
        kind: CaseKind::Semantic,
    },
];

// ── Scoring ─────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct Score {
    label: String,
    /// Rank the right record came back at, 1-based; `None` is not in the top
    /// [`LIMIT`] at all.
    ranks: Vec<(CaseKind, Option<usize>)>,
    latencies: Vec<Duration>,
    /// Resident set after this arm's models were loaded, and the rise from before.
    rss_after_kb: Option<u64>,
    rss_delta_kb: Option<i64>,
}

impl Score {
    fn of_kind(&self, kind: Option<CaseKind>) -> Vec<Option<usize>> {
        self.ranks
            .iter()
            .filter(|(k, _)| kind.is_none_or(|want| *k == want))
            .map(|(_, r)| *r)
            .collect()
    }

    /// Share of cases whose right answer came back at all.
    fn found(&self, kind: Option<CaseKind>) -> f64 {
        share(&self.of_kind(kind), |r| r.is_some())
    }

    /// Share of cases whose right answer came back *first*. The number that
    /// matters most: the model reads the top of the list and stops.
    fn top1(&self, kind: Option<CaseKind>) -> f64 {
        share(&self.of_kind(kind), |r| r == &Some(1))
    }

    /// Mean reciprocal rank — the metric that distinguishes "second" from "ninth"
    /// where `found` treats them alike.
    fn mrr(&self, kind: Option<CaseKind>) -> f64 {
        let ranks = self.of_kind(kind);
        if ranks.is_empty() {
            return 0.0;
        }
        let total: f64 = ranks
            .iter()
            .map(|r| r.map_or(0.0, |rank| 1.0 / rank as f64))
            .sum();
        total / ranks.len() as f64
    }

    /// Median and worst query time. Median rather than mean: one cold-cache
    /// outlier would otherwise set the number for the whole arm.
    fn latency(&self) -> Option<(Duration, Duration)> {
        if self.latencies.is_empty() {
            return None;
        }
        let mut sorted = self.latencies.clone();
        sorted.sort();
        Some((sorted[sorted.len() / 2], *sorted.last().unwrap()))
    }
}

fn share<T>(items: &[T], pred: impl Fn(&T) -> bool) -> f64 {
    if items.is_empty() {
        return 0.0;
    }
    items.iter().filter(|i| pred(i)).count() as f64 / items.len() as f64
}

/// Resident set size in KB, from `/proc/self/status`.
///
/// ⚠️ Linux only, and `None` elsewhere rather than a fabricated zero. The number
/// this measurement exists to inform is a Linux box's memory budget, so a platform
/// that cannot report it should say so rather than report something.
fn rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))
        .and_then(|v| v.split_whitespace().next())
        .and_then(|n| n.parse().ok())
}

// ── The run ─────────────────────────────────────────────────────────────────

pub async fn run(cache_dir: PathBuf) {
    println!("\n─── retrieval scorecard ───");
    println!(
        "corpus: {} notes, {} journal entries · {} cases ({} lexical, {} semantic) · top-{LIMIT}\n",
        NOTES.len(),
        JOURNAL.len(),
        CASES.len(),
        CASES.iter().filter(|c| c.kind == CaseKind::Lexical).count(),
        CASES
            .iter()
            .filter(|c| c.kind == CaseKind::Semantic)
            .count(),
    );

    // A config with only the two types the fixture populates. Leaving finances and
    // routines on would put empty tables in the catalogue, and every arm would
    // spend a query per case failing to search them — measurable, and measuring
    // nothing anyone cares about.
    let config = fixture_config();

    let mut scores: Vec<Score> = Vec::new();

    // ── Arm 1: keyword only, the behaviour that shipped before Phase C ──
    let baseline = match scratch_corpus().await {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("could not build the fixture corpus: {e}");
            return;
        }
    };
    scores.push(score_arm(&baseline.1, &config, "keyword only", Retrievers::default()).await);
    drop(baseline);

    // ── One embedder per pass, each in its own database ──
    //
    // ⚠️ A fresh database per embedder is not tidiness. `DEFINE INDEX ... HNSW
    // DIMENSION n` fixes the width when the index is created, so a 384-wide model
    // and a 768-wide one cannot share a table — and the second would fail to
    // insert rather than produce bad results, which at least fails loudly.
    for embed_model in embed_models() {
        let (dir, db) = match scratch_corpus().await {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("could not build the fixture corpus: {e}");
                return;
            }
        };

        let rss_before_embedder = rss_kb();
        let Some(embedder) = load_embedder(&db, &embed_model, cache_dir.clone()).await else {
            continue;
        };
        let rss_with_embedder = rss_kb();

        if let Err(e) = vector_store::sweep(&db, &config, &embedder).await {
            eprintln!("could not build the vector index for {embed_model}: {e}");
            continue;
        }

        let search = VectorSearch {
            db: &db,
            config: &config,
            embedder: &embedder,
        };
        let fused = Retrievers {
            semantic: Some(&search as &dyn SemanticSearch),
            reranker: None,
        };
        let mut fused_score =
            score_arm(&db, &config, &format!("fused [{embed_model}]"), fused).await;
        fused_score.rss_after_kb = rss_with_embedder;
        fused_score.rss_delta_kb = delta(rss_before_embedder, rss_with_embedder);
        scores.push(fused_score);

        for model in rerank_models() {
            let rss_before = rss_kb();
            let reranker = match Reranker::load(&model, cache_dir.clone()) {
                Ok(r) => r,
                Err(e) => {
                    println!("  ⚠ {model}: could not load ({e}) — skipped");
                    continue;
                }
            };
            let rss_after = rss_kb();
            let service = RerankService {
                reranker: &reranker,
            };
            let arm = Retrievers {
                semantic: Some(&search as &dyn SemanticSearch),
                reranker: Some(&service as &dyn Rerank),
            };
            let mut score = score_arm(&db, &config, &format!("  + {model}"), arm).await;
            score.rss_after_kb = rss_after;
            score.rss_delta_kb = delta(rss_before, rss_after);
            scores.push(score);
        }
        drop(dir);
    }

    report(&scores);
}

/// A scratch database holding the fixture corpus.
///
/// The `TempDir` is returned alongside rather than dropped here: dropping it
/// deletes the directory the database is still open on.
async fn scratch_corpus() -> Result<(tempfile::TempDir, Database), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let db = seed_corpus(dir.path()).await?;
    Ok((dir, db))
}

fn delta(before: Option<u64>, after: Option<u64>) -> Option<i64> {
    Some(after? as i64 - before? as i64)
}

/// Which rerankers to measure. Every offered model by default.
///
/// `OMNI_BENCH_RERANK_MODELS` narrows it to a comma-separated subset, which is what
/// makes this runnable on a machine that cannot hold the 2.3 GB one.
fn rerank_models() -> Vec<String> {
    model_list("OMNI_BENCH_RERANK_MODELS", RERANK_MODEL_VALUES)
}

/// Which embedders to measure. **Only the configured default**, unless asked.
///
/// Asymmetric with [`rerank_models`] on purpose: the reranker is the open question
/// this bench exists to settle, while the embedder was settled in C1 and each extra
/// one costs a fresh index over the whole corpus. `OMNI_BENCH_EMBED_MODELS` widens
/// it when the embedder is what is being questioned.
fn embed_models() -> Vec<String> {
    let default = ConfigKey::AssistantEmbedModel.default_value();
    let default = default.as_text().unwrap_or("bge-small-en-v1.5");
    model_list("OMNI_BENCH_EMBED_MODELS", &[default])
}

fn model_list(var: &str, fallback: &[&str]) -> Vec<String> {
    match std::env::var(var) {
        Ok(list) => list
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        Err(_) => fallback.iter().map(|s| (*s).to_string()).collect(),
    }
}

async fn load_embedder(db: &Database, model: &str, cache_dir: PathBuf) -> Option<Embedder> {
    let embedder = match Embedder::load(model, cache_dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("could not load the embedding model: {e}");
            eprintln!("(is scripts/fetch-onnxruntime.sh sourced?)");
            return None;
        }
    };
    if let Err(e) = vector_store::init_schema(db, embedder.dim()).await {
        eprintln!("could not define the vector index: {e}");
        return None;
    }
    Some(embedder)
}

/// Only journal and notes, because the fixture populates those and nothing else.
///
/// Leaving the other features on would put empty tables in the catalogue, and every
/// arm would spend a query per case searching them — measurable work that measures
/// nothing. Built from defaults rather than from the host's real config, which this
/// run deliberately never loads.
fn fixture_config() -> ResolvedConfig {
    let mut global = ConfigMap::new();
    for feature in [Feature::Routines, Feature::Finances, Feature::AutoImport] {
        global.insert(feature.key(), ConfigValue::Bool(false));
    }
    ResolvedConfig::new(global, ConfigMap::new())
}

async fn seed_corpus(dir: &std::path::Path) -> Result<Database, String> {
    let path = dir.join("bench.db");
    let db = omni_me_core::db::connect(path.to_str().ok_or("non-utf8 scratch path")?)
        .await
        .map_err(|e| e.to_string())?;
    NotesProjection
        .init_schema(&db)
        .await
        .map_err(|e| e.to_string())?;

    for (id, title, body) in NOTES {
        db.query(
            "CREATE type::record('generic_notes', $id) SET title = $title, raw_text = $body,
             tags = [], created_at = time::now(), updated_at = time::now()",
        )
        .bind(("id", id.to_string()))
        .bind(("title", title.to_string()))
        .bind(("body", body.to_string()))
        .await
        .map_err(|e| format!("seeding {id}: {e}"))?;
    }
    for (id, date, body) in JOURNAL {
        db.query(
            "CREATE type::record('journal_entries', $id) SET journal_id = $id, date = $date,
             raw_text = $body, tags = [], closed = false, complete = false,
             created_at = time::now(), updated_at = time::now()",
        )
        .bind(("id", id.to_string()))
        .bind(("date", date.to_string()))
        .bind(("body", body.to_string()))
        .await
        .map_err(|e| format!("seeding {id}: {e}"))?;
    }
    Ok(db)
}

async fn score_arm(
    db: &Database,
    config: &ResolvedConfig,
    label: &str,
    retrievers: Retrievers<'_>,
) -> Score {
    let mut score = Score {
        label: label.to_string(),
        ..Default::default()
    };
    for case in CASES {
        let args = json!({ "query": case.question, "limit": LIMIT });
        let started = Instant::now();
        let out = dispatch_with(db, config, "search", &args, retrievers).await;
        score.latencies.push(started.elapsed());
        score
            .ranks
            .push((case.kind, rank_of(&out, case.expect_type, case.expect_id)));
    }
    score
}

/// Where the right record landed, 1-based.
fn rank_of(out: &Value, expect_type: &str, expect_id: &str) -> Option<usize> {
    out["results"]
        .as_array()?
        .iter()
        .position(|r| r["record_type"] == expect_type && r["id"] == expect_id)
        .map(|i| i + 1)
}

fn report(scores: &[Score]) {
    println!(
        "{:<34} {:>7} {:>7} {:>7} {:>9} {:>9} {:>10} {:>9}",
        "arm", "found", "top-1", "MRR", "MRR lex", "MRR sem", "median", "RSS"
    );
    for s in scores {
        let median = s
            .latency()
            .map(|(m, _)| format!("{}ms", m.as_millis()))
            .unwrap_or_else(|| "—".into());
        let rss = match (s.rss_after_kb, s.rss_delta_kb) {
            (Some(after), Some(d)) => format!("{}M +{}M", after / 1024, d / 1024),
            (Some(after), None) => format!("{}M", after / 1024),
            _ => "—".into(),
        };
        println!(
            "{:<34} {:>6.0}% {:>6.0}% {:>7.3} {:>9.3} {:>9.3} {:>10} {:>9}",
            s.label,
            s.found(None) * 100.0,
            s.top1(None) * 100.0,
            s.mrr(None),
            s.mrr(Some(CaseKind::Lexical)),
            s.mrr(Some(CaseKind::Semantic)),
            median,
            rss,
        );
    }

    println!("\nworst-case query time");
    for s in scores {
        if let Some((_, worst)) = s.latency() {
            println!("  {:<34} {}ms", s.label, worst.as_millis());
        }
    }

    println!(
        "\n⚠ RSS is this whole process, and the delta is the rise across one model's \
         load. Allocators rarely return freed pages, so a model measured after another\n  \
         reads smaller than it is. Treat the deltas as a floor, and the largest \
         absolute figure as the honest one for that model."
    );
    println!(
        "⚠ Timings are this machine's. The deployment host has two cores; per-query \
         cost there scales with the reranker and barely at all with the embedder."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A case pointing at a record that does not exist would score zero forever
    /// and read as a retrieval failure.
    #[test]
    fn every_case_names_a_record_that_is_in_the_corpus() {
        for case in CASES {
            let present = match case.expect_type {
                "note" => NOTES.iter().any(|(id, _, _)| *id == case.expect_id),
                "journal" => JOURNAL.iter().any(|(id, _, _)| *id == case.expect_id),
                other => panic!("{}: unknown record type {other}", case.question),
            };
            assert!(
                present,
                "{}: expects {} which is not in the corpus",
                case.question, case.expect_id
            );
        }
    }

    #[test]
    fn fixture_ids_are_unique() {
        let mut ids: Vec<&str> = NOTES
            .iter()
            .map(|(id, _, _)| *id)
            .chain(JOURNAL.iter().map(|(id, _, _)| *id))
            .collect();
        let before = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(before, ids.len(), "two fixture records share an id");
    }

    /// Both columns have to be populated or the split scorecard is decoration.
    #[test]
    fn both_case_kinds_are_represented() {
        assert!(CASES.iter().any(|c| c.kind == CaseKind::Lexical));
        assert!(CASES.iter().any(|c| c.kind == CaseKind::Semantic));
    }

    /// A `Semantic` case whose distinctive words appear verbatim in its answer is
    /// mislabelled, and would quietly inflate the column that exists to show what
    /// keyword search *cannot* do.
    #[test]
    fn semantic_cases_share_no_distinctive_word_with_their_answer() {
        // Function words carry no retrieval signal, so an overlap on them is not
        // an overlap. Everything else counts.
        const STOP: &[&str] = &[
            "the", "a", "an", "my", "i", "did", "how", "much", "when", "is", "was", "has", "have",
            "been", "at", "in", "on", "for", "to", "of", "and", "with", "who", "it", "get",
            "getting", "from", "day", "back", "up", "out", "worse", "that",
        ];
        for case in CASES.iter().filter(|c| c.kind == CaseKind::Semantic) {
            let answer = match case.expect_type {
                "note" => NOTES
                    .iter()
                    .find(|(id, _, _)| *id == case.expect_id)
                    .map(|(_, t, b)| format!("{t} {b}")),
                _ => JOURNAL
                    .iter()
                    .find(|(id, _, _)| *id == case.expect_id)
                    .map(|(_, d, b)| format!("{d} {b}")),
            }
            .expect("case names a record in the corpus")
            .to_lowercase();

            // ⚠️ Whole words, not substrings. A substring check reports "me"
            // overlapping "kilometres" and would push the fixture toward avoiding
            // innocent letter sequences instead of avoiding shared terms.
            let answer_words: Vec<&str> = answer.split(|c: char| !c.is_alphanumeric()).collect();

            for word in case
                .question
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
                .collect::<Vec<_>>()
            {
                if STOP.contains(&word) {
                    continue;
                }
                assert!(
                    !answer_words.contains(&word),
                    "{:?} is labelled Semantic but shares {word:?} with {}",
                    case.question,
                    case.expect_id
                );
            }
        }
    }
}
