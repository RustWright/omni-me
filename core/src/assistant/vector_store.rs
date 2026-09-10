//! The vector side of retrieval: one side table, one HNSW index, one sweep.
//!
//! Rationale in `docs/src/assistant.md` § retrieval. The short version is that
//! vectors live beside the record tables rather than inside them, because a long
//! journal entry needs several vectors and a column can only hold one — and because
//! the record tables are shared with a client that will never run a model.

use serde_json::Value;
use surrealdb::types::SurrealValue;

use super::catalog::{self, CatalogEntry};
use super::chunk;
use super::embedding::{Embedder, EmbeddingError};
use super::fusion::RecordKey;
use crate::config::ResolvedConfig;
use crate::db::{Database, DbError};

/// Table holding one row per chunk of one record.
const TABLE: &str = "record_embeddings";

/// `EF` for the HNSW search — how many candidates the graph walk keeps in flight.
///
/// ⚠️ **Both this and `k` must be integers in the query text.** SurrealDB's
/// `<|K,EF|>` operator dispatches on the *shape* of its arguments: two integers is
/// the approximate/HNSW path, but `<|K,COSINE|>` is a brute-force scan that parses
/// perfectly well and silently never touches the index. See
/// `syn/parser/expression.rs:309` in surrealdb-core 3.0.4.
const SEARCH_EF: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum VectorError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Embedding(#[from] EmbeddingError),
}

/// What one sweep did, so a caller can log it rather than guess.
///
/// ⚠️ Reports `skipped` explicitly rather than only counting work done: a sweep that
/// silently embedded nothing and a sweep that correctly found nothing to do look
/// identical otherwise.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SweepReport {
    pub scanned: usize,
    pub embedded: usize,
    pub skipped: usize,
    pub removed: usize,
    /// Types that could not be swept at all, counted rather than swallowed.
    pub failed: usize,
}

/// Create the table and its vector index.
///
/// `dim` comes from the loaded model, never a constant: the index fixes `DIMENSION`
/// at definition time, so a model of a different width would produce vectors the
/// index cannot accept.
pub async fn init_schema(db: &Database, dim: usize) -> Result<(), DbError> {
    let sql = format!(
        "DEFINE TABLE IF NOT EXISTS {TABLE} SCHEMAFULL;
         DEFINE FIELD IF NOT EXISTS record_type ON {TABLE} TYPE string;
         DEFINE FIELD IF NOT EXISTS record_id ON {TABLE} TYPE string;
         DEFINE FIELD IF NOT EXISTS chunk_index ON {TABLE} TYPE int;
         DEFINE FIELD IF NOT EXISTS text ON {TABLE} TYPE string;
         DEFINE FIELD IF NOT EXISTS text_hash ON {TABLE} TYPE string;
         DEFINE FIELD IF NOT EXISTS model ON {TABLE} TYPE string;
         DEFINE FIELD IF NOT EXISTS vector ON {TABLE} TYPE array<float>;
         DEFINE INDEX IF NOT EXISTS idx_re_record ON {TABLE} FIELDS record_type, record_id;
         -- ⚠️ `DISTANCE`, not `DIST`. The latter is not a keyword and the statement
         -- fails to parse; verified against surrealdb-core 3.0.4's own parser.
         DEFINE INDEX IF NOT EXISTS idx_re_hnsw ON {TABLE}
             FIELDS vector HNSW DIMENSION {dim} DISTANCE COSINE TYPE F32 EFC 150 M 12;"
    );
    db.query(&sql).await?;
    Ok(())
}

/// Drop every stored vector, returning how many rows went.
///
/// The escape hatch for the content hash: after an embedding-model change the
/// stored vectors are the right shape and the wrong meaning, and the sweep's guard
/// would happily skip all of them. `is_current` checks the model name for exactly
/// that reason, so this is for a corrupted index or a forced rebuild.
pub async fn clear(db: &Database) -> Result<usize, DbError> {
    let mut resp = db
        .query(format!("DELETE FROM {TABLE} RETURN BEFORE"))
        .await?;
    let gone: Vec<Value> = resp.take(0)?;
    Ok(gone.len())
}

/// One row as stored, minus the vector.
#[derive(Debug, SurrealValue)]
struct StoredChunk {
    record_id: Option<String>,
    text_hash: Option<String>,
    model: Option<String>,
}

/// A row read out of a record table, ready to embed.
#[derive(Debug, SurrealValue)]
struct SourceRow {
    id: Option<String>,
    text: Option<String>,
}

/// Bring the index up to date with the record tables.
///
/// Deliberately **not** a `Projection`. A projection's `apply` fires per event, and
/// journal autosave emits dozens of `journal_entry_updated` events for one day's
/// entry — a replay would re-embed the same text dozens of times, slowest exactly on
/// the cold start where it hurts most. Sweeping materialized rows against a content
/// hash costs one pass and is naturally idempotent.
pub async fn sweep(
    db: &Database,
    config: &ResolvedConfig,
    embedder: &Embedder,
) -> Result<SweepReport, VectorError> {
    let mut report = SweepReport::default();
    for entry in catalog::visible(config) {
        // One type failing must not abandon the rest. A feature can be enabled
        // before its projection has materialized its tables — normal on a cold
        // start — and aborting there would leave the whole index unbuilt because
        // of a type the question was never about. Same posture as `store::search`.
        if let Err(e) = sweep_one(db, entry, embedder, &mut report).await {
            tracing::warn!(record_type = entry.name, error = %e, "could not sweep this type");
            report.failed += 1;
        }
    }
    Ok(report)
}

async fn sweep_one(
    db: &Database,
    entry: &CatalogEntry,
    embedder: &Embedder,
    report: &mut SweepReport,
) -> Result<(), VectorError> {
    // Every text field concatenated, in catalogue order: a note's title carries
    // meaning its body often omits, and embedding the body alone loses it.
    let text_expr = entry
        .text_fields
        .iter()
        .map(|f| format!("string::concat({f}, '\n')"))
        .collect::<Vec<_>>()
        .join(" + ");

    let sql = format!(
        "SELECT meta::id(id) AS id, ({text_expr}) AS text FROM {}",
        entry.table
    );
    let mut resp = db.query(&sql).await.map_err(DbError::from)?;
    let rows: Vec<SourceRow> = resp.take(0).map_err(DbError::from)?;

    for row in rows {
        let Some(record_id) = row.id else { continue };
        let text = row.text.unwrap_or_default();
        report.scanned += 1;

        let hash = content_hash(&text);
        if is_current(db, entry.name, &record_id, &hash, embedder.name()).await? {
            report.skipped += 1;
            continue;
        }

        // Delete before insert rather than upserting per chunk: editing a long note
        // down to a short one leaves fewer chunks, and the vanished tail would
        // otherwise stay in the index forever, matching a passage that no longer
        // exists anywhere in the record.
        report.removed += delete_chunks(db, entry.name, &record_id).await?;

        let chunks = chunk::chunk(&text);
        if chunks.is_empty() {
            continue;
        }
        let vectors = embedder.embed_passages(chunks.clone()).await?;
        for (i, (piece, vector)) in chunks.into_iter().zip(vectors).enumerate() {
            insert_chunk(
                db,
                entry.name,
                &record_id,
                i,
                &piece,
                &hash,
                embedder.name(),
                vector,
            )
            .await?;
        }
        report.embedded += 1;
    }
    Ok(())
}

/// Is this record already indexed, under this exact text and this exact model?
///
/// ⚠️ The model name is half the check, not decoration. Two models of the *same*
/// width produce vectors in unrelated spaces, so a config swap between them leaves
/// an index that is the right shape and entirely meaningless. Width cannot detect
/// that; the name can.
async fn is_current(
    db: &Database,
    record_type: &str,
    record_id: &str,
    hash: &str,
    model: &str,
) -> Result<bool, DbError> {
    let sql = format!(
        "SELECT record_id, text_hash, model FROM {TABLE}
         WHERE record_type = $t AND record_id = $i LIMIT 1"
    );
    let mut resp = db
        .query(&sql)
        .bind(("t", record_type.to_string()))
        .bind(("i", record_id.to_string()))
        .await?;
    let rows: Vec<StoredChunk> = resp.take(0)?;
    Ok(rows
        .first()
        .is_some_and(|r| r.text_hash.as_deref() == Some(hash) && r.model.as_deref() == Some(model)))
}

async fn delete_chunks(
    db: &Database,
    record_type: &str,
    record_id: &str,
) -> Result<usize, DbError> {
    let sql =
        format!("DELETE FROM {TABLE} WHERE record_type = $t AND record_id = $i RETURN BEFORE");
    let mut resp = db
        .query(&sql)
        .bind(("t", record_type.to_string()))
        .bind(("i", record_id.to_string()))
        .await?;
    let gone: Vec<Value> = resp.take(0)?;
    Ok(gone.len())
}

#[allow(clippy::too_many_arguments)]
async fn insert_chunk(
    db: &Database,
    record_type: &str,
    record_id: &str,
    chunk_index: usize,
    text: &str,
    hash: &str,
    model: &str,
    vector: Vec<f32>,
) -> Result<(), DbError> {
    // A deterministic id, so a re-run converges on the same rows instead of
    // accumulating duplicates that would each match a query separately.
    let id = format!("{record_type}|{record_id}|{chunk_index}");
    let vector: Vec<f64> = vector.into_iter().map(f64::from).collect();
    let sql = format!(
        "UPSERT type::record('{TABLE}', $id) SET
            record_type = $t, record_id = $i, chunk_index = $n,
            text = $x, text_hash = $h, model = $m, vector = $v"
    );
    db.query(&sql)
        .bind(("id", id))
        .bind(("t", record_type.to_string()))
        .bind(("i", record_id.to_string()))
        .bind(("n", chunk_index as i64))
        .bind(("x", text.to_string()))
        .bind(("h", hash.to_string()))
        .bind(("m", model.to_string()))
        .bind(("v", vector))
        .await?;
    Ok(())
}

/// One nearest-neighbour hit, before fusion.
#[derive(Debug, Clone)]
pub struct VectorHit {
    pub key: RecordKey,
    pub text: String,
    pub distance: f64,
}

#[derive(Debug, SurrealValue)]
struct RawKnn {
    record_type: Option<String>,
    record_id: Option<String>,
    text: Option<String>,
    dist: Option<f64>,
}

/// Nearest chunks to a question, best first, deduplicated to one per record.
///
/// Type filtering happens in Rust rather than as a `WHERE` clause beside the KNN
/// operator: narrowing an approximate index by a predicate it does not know about
/// silently changes what the graph walk returns.
pub async fn knn_search(
    db: &Database,
    config: &ResolvedConfig,
    embedder: &Embedder,
    query: &str,
    limit: usize,
) -> Result<Vec<VectorHit>, VectorError> {
    let vector: Vec<f64> = embedder
        .embed_query(query)
        .await?
        .into_iter()
        .map(f64::from)
        .collect();

    // Over-fetch: several chunks of one record can occupy the top, and the visible
    // catalogue may exclude a type entirely. Both shrink the useful result.
    let fetch = (limit * 4).max(limit + SEARCH_EF.min(32));
    let sql = format!(
        "SELECT record_type, record_id, text, vector::distance::knn() AS dist
         FROM {TABLE}
         WHERE vector <|{fetch},{SEARCH_EF}|> $q
         ORDER BY dist ASC"
    );
    let mut resp = db
        .query(&sql)
        .bind(("q", vector))
        .await
        .map_err(DbError::from)?;
    let raw: Vec<RawKnn> = resp.take(0).map_err(DbError::from)?;

    let visible: Vec<&str> = catalog::visible(config).iter().map(|e| e.name).collect();
    let mut out: Vec<VectorHit> = Vec::new();
    for r in raw {
        let (Some(record_type), Some(record_id)) = (r.record_type, r.record_id) else {
            continue;
        };
        if !visible.contains(&record_type.as_str()) {
            continue;
        }
        // Best chunk wins; a record matching in three places is still one result,
        // and returning it three times would crowd out three other records.
        if out
            .iter()
            .any(|h| h.key.record_type == record_type && h.key.id == record_id)
        {
            continue;
        }
        out.push(VectorHit {
            key: RecordKey {
                record_type,
                id: record_id,
            },
            text: r.text.unwrap_or_default(),
            distance: r.dist.unwrap_or(f64::MAX),
        });
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

/// The [`SemanticSearch`] implementation, bound to a database and a loaded model.
///
/// Holds borrows rather than owning: it lives for one question, alongside the
/// `Session` that consults it.
pub struct VectorSearch<'a> {
    pub db: &'a Database,
    pub config: &'a ResolvedConfig,
    pub embedder: &'a Embedder,
}

#[async_trait::async_trait]
impl super::retrieval::SemanticSearch for VectorSearch<'_> {
    async fn search(&self, query: &str, limit: usize) -> Vec<super::retrieval::SemanticHit> {
        match knn_search(self.db, self.config, self.embedder, query, limit).await {
            Ok(hits) => hits
                .into_iter()
                .map(|h| super::retrieval::SemanticHit {
                    key: h.key,
                    text: h.text,
                })
                .collect(),
            // Degrade to keyword-only rather than failing the question. Loud in the
            // log, silent to the model — it cannot act on this and would only burn
            // a turn trying.
            Err(e) => {
                tracing::warn!(error = %e, "semantic search failed; keyword results only");
                Vec::new()
            }
        }
    }
}

/// FNV-1a over the record's text, hex-encoded.
///
/// Chosen over `DefaultHasher` because that one's output is explicitly not stable
/// across Rust releases — a toolchain upgrade would invalidate every stored hash and
/// silently re-embed the entire corpus. FNV-1a is a fixed algorithm and always will be.
fn content_hash(text: &str) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = OFFSET;
    for byte in text.as_bytes() {
        h ^= u64::from(*byte);
        h = h.wrapping_mul(PRIME);
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::embedding::DEFAULT_EMBED_MODEL;
    use crate::config::ConfigMap;
    use crate::events::{NotesProjection, Projection, RoutinesProjection};
    use std::sync::OnceLock;

    /// One model for the whole test binary.
    ///
    /// ⚠️ These tests reach the network on a cold cache — fastembed downloads
    /// ~133 MB the first time. Loading once rather than per test keeps that to a
    /// single fetch and a single ONNX session.
    fn shared_embedder() -> &'static Embedder {
        static EMBEDDER: OnceLock<Embedder> = OnceLock::new();
        EMBEDDER.get_or_init(|| {
            let cache = std::env::var("FASTEMBED_CACHE_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::env::temp_dir().join("omni-fastembed-test"));
            Embedder::load(DEFAULT_EMBED_MODEL, cache).expect(
                "could not load the embedding model — is scripts/fetch-onnxruntime.sh sourced?",
            )
        })
    }

    fn config() -> ResolvedConfig {
        ResolvedConfig::new(ConfigMap::new(), ConfigMap::new())
    }

    async fn test_db(dim: usize) -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vec.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        NotesProjection.init_schema(&db).await.unwrap();
        RoutinesProjection.init_schema(&db).await.unwrap();
        init_schema(&db, dim).await.unwrap();
        std::mem::forget(dir);
        db
    }

    async fn seed_note(db: &Database, id: &str, title: &str, body: &str) {
        db.query(
            "CREATE type::record('generic_notes', $id) SET title = $t, raw_text = $b,
             tags = [], created_at = time::now(), updated_at = time::now()",
        )
        .bind(("id", id.to_string()))
        .bind(("t", title.to_string()))
        .bind(("b", body.to_string()))
        .await
        .unwrap();
    }

    /// The case that fails today and is the entire point of the phase: no word
    /// overlaps between the query and the note, so BM25 returns nothing.
    #[tokio::test]
    async fn a_synonym_query_finds_a_note_that_shares_no_words() {
        let embedder = shared_embedder();
        let db = test_db(embedder.dim()).await;
        seed_note(
            &db,
            "01JKVEC0000000000000000001",
            "Housing",
            "The landlord bumped the rate again this spring.",
        )
        .await;
        seed_note(
            &db,
            "01JKVEC0000000000000000002",
            "Sourdough",
            "The starter doubled overnight and the crumb was open.",
        )
        .await;

        let report = sweep(&db, &config(), embedder).await.unwrap();
        assert_eq!(report.embedded, 2, "{report:?}");

        let hits = knn_search(&db, &config(), embedder, "rent increase", 2)
            .await
            .unwrap();

        assert!(!hits.is_empty(), "semantic search returned nothing");
        assert_eq!(
            hits[0].key.id, "01JKVEC0000000000000000001",
            "the housing note should outrank the bread one: {hits:?}"
        );
    }

    /// Autosave churn is why this is a sweep and not a projection; the guard is
    /// what makes re-running it cheap.
    #[tokio::test]
    async fn a_second_sweep_embeds_nothing() {
        let embedder = shared_embedder();
        let db = test_db(embedder.dim()).await;
        seed_note(&db, "01JKVEC0000000000000000003", "A", "a quiet day").await;

        let first = sweep(&db, &config(), embedder).await.unwrap();
        assert_eq!(first.embedded, 1);

        let second = sweep(&db, &config(), embedder).await.unwrap();
        assert_eq!(second.embedded, 0, "re-embedded unchanged text: {second:?}");
        assert_eq!(second.skipped, 1, "{second:?}");
    }

    /// Editing a long record down to a short one must not strand the vanished
    /// tail in the index, still matching text that no longer exists.
    #[tokio::test]
    async fn shrinking_a_record_drops_its_extra_chunks() {
        let embedder = shared_embedder();
        let db = test_db(embedder.dim()).await;
        let long = "the landlord raised the rent again this spring. ".repeat(120);
        seed_note(&db, "01JKVEC0000000000000000004", "Long", &long).await;
        sweep(&db, &config(), embedder).await.unwrap();

        let before = count_chunks(&db).await;
        assert!(before > 1, "expected a multi-chunk record, got {before}");

        db.query("UPDATE type::record('generic_notes', $id) SET raw_text = 'short now'")
            .bind(("id", "01JKVEC0000000000000000004"))
            .await
            .unwrap();
        sweep(&db, &config(), embedder).await.unwrap();

        assert_eq!(count_chunks(&db).await, 1, "stale chunks survived the edit");
    }

    /// A type whose projection has not run yet must not abandon the whole sweep.
    ///
    /// Regression: the first version propagated the error, so a cold-start agent
    /// with `routines` enabled but unmaterialized indexed *nothing at all* — and
    /// the failure named a table the question was never about.
    #[tokio::test]
    async fn a_missing_table_is_counted_not_fatal() {
        let embedder = shared_embedder();
        // Deliberately only the notes tables — `routine_groups` will be absent.
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::connect(dir.path().join("v.db").to_str().unwrap())
            .await
            .unwrap();
        NotesProjection.init_schema(&db).await.unwrap();
        init_schema(&db, embedder.dim()).await.unwrap();
        seed_note(&db, "01JKVEC0000000000000000005", "A", "a quiet day").await;

        let report = sweep(&db, &config(), embedder).await.unwrap();

        assert_eq!(
            report.failed, 1,
            "the missing type went unreported: {report:?}"
        );
        assert_eq!(
            report.embedded, 1,
            "the healthy type was skipped: {report:?}"
        );
    }

    /// The silent failure this phase is most exposed to.
    ///
    /// `<|10,COSINE|>` parses perfectly and brute-force scans; only `<|K,EF|>` —
    /// two integers — dispatches to the HNSW index. Nothing about the result
    /// distinguishes them until the corpus is large enough to be slow, so the
    /// query plan is asserted directly.
    #[tokio::test]
    async fn the_knn_query_uses_the_hnsw_index() {
        let embedder = shared_embedder();
        let db = test_db(embedder.dim()).await;
        seed_note(&db, "01JKVEC0000000000000000006", "A", "a quiet day").await;
        sweep(&db, &config(), embedder).await.unwrap();

        let vector: Vec<f64> = embedder
            .embed_query("quiet")
            .await
            .unwrap()
            .into_iter()
            .map(f64::from)
            .collect();
        let sql =
            format!("SELECT record_id FROM {TABLE} WHERE vector <|4,{SEARCH_EF}|> $q EXPLAIN");
        let mut resp = db.query(&sql).bind(("q", vector)).await.unwrap();
        let plan: Vec<Value> = resp.take(0).unwrap();
        let rendered = serde_json::to_string(&plan).unwrap();

        assert!(
            rendered.contains("idx_re_hnsw"),
            "the KNN query did not use the HNSW index; plan was {rendered}"
        );
    }

    async fn count_chunks(db: &Database) -> usize {
        let mut resp = db
            .query(format!("SELECT count() AS n FROM {TABLE} GROUP ALL"))
            .await
            .unwrap();
        let rows: Vec<Value> = resp.take(0).unwrap();
        rows.first().and_then(|v| v["n"].as_u64()).unwrap_or(0) as usize
    }

    #[test]
    fn the_hash_is_stable_and_distinguishes_edits() {
        assert_eq!(content_hash("rent increase"), content_hash("rent increase"));
        assert_ne!(
            content_hash("rent increase"),
            content_hash("rent increases")
        );
        // Pinned, so a refactor that changes the algorithm fails loudly here rather
        // than silently re-embedding every record on the next sweep.
        assert_eq!(content_hash(""), "cbf29ce484222325");
    }
}
