//! The off-box backup: blobs hourly, a database export and the config nightly, through rclone's
//! crypt layer to an S3-compatible bucket. Design and rationale: `docs/src/backup.md`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use rand::seq::SliceRandom;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::db::{Database, queries};
use crate::jobs::runs::{RunObserver, RunOutcome, RunRecord, RunStore, bounded_detail};

pub const JOB: &str = crate::jobs::BACKUP;
pub const SUBJECT_BLOBS: &str = "blobs";
pub const SUBJECT_NIGHTLY: &str = "nightly";
pub const TICK: Duration = Duration::from_secs(3600);

/// Exports and config copies older than this are pruned. Above the 7-day lock on their prefixes.
const KEEP: &str = "30d";
/// The free tier is 10 GB-month of storage; past this the run asks for attention.
const WARN_BYTES: u64 = 8 * 1024 * 1024 * 1024;
/// Blobs downloaded and re-hashed each night, so the backup is proven readable, not assumed.
const RESTORE_SAMPLE: usize = 3;
/// A database export under this fraction of the previous one is refused as truncated.
const MIN_EXPORT_RATIO: f64 = 0.5;

const SECRET_KEYS: [&str; 5] = [
    "backup_endpoint",
    "backup_access_key_id",
    "backup_secret_access_key",
    "backup_bucket",
    "backup_key",
];

/// Where the backup goes and how it is encrypted, from the `backup_*` secrets.
#[derive(Clone)]
pub struct BackupConfig {
    endpoint: String,
    access_key_id: String,
    secret_access_key: String,
    bucket: String,
    /// `""` on live, `"dev/"` on dev: dev's objects sit outside the bucket's lock rules.
    prefix: String,
    key: String,
}

impl BackupConfig {
    /// `None` when no `backup_*` secret is set. Some but not all is an error naming the missing ones,
    /// so a half-provisioned box says so instead of silently backing nothing up.
    pub fn from_secrets(secrets: &HashMap<String, String>) -> Result<Option<Self>, String> {
        let get = |k: &str| {
            secrets
                .get(k)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let missing: Vec<&str> = SECRET_KEYS
            .iter()
            .copied()
            .filter(|k| get(k).is_none())
            .collect();
        if missing.len() == SECRET_KEYS.len() {
            return Ok(None);
        }
        if !missing.is_empty() {
            return Err(format!(
                "backup secrets incomplete, missing: {}",
                missing.join(", ")
            ));
        }
        let prefix = get("backup_prefix").unwrap_or_default();
        if prefix.starts_with('/') || (!prefix.is_empty() && !prefix.ends_with('/')) {
            return Err(format!(
                "backup_prefix must be empty or end in '/', got {prefix:?}"
            ));
        }
        let endpoint = get("backup_endpoint").expect("checked above");
        if !endpoint.starts_with("https://") {
            return Err("backup_endpoint must be an https:// address".into());
        }
        Ok(Some(Self {
            endpoint,
            access_key_id: get("backup_access_key_id").expect("checked above"),
            secret_access_key: get("backup_secret_access_key").expect("checked above"),
            bucket: get("backup_bucket").expect("checked above"),
            prefix,
            key: get("backup_key").expect("checked above"),
        }))
    }

    fn root(&self) -> String {
        format!("r2:{}/{}", self.bucket, self.prefix)
    }
}

/// The paths a run reads and writes. Staging must be on the data volume: a database export
/// can be larger than the container's own filesystem allows.
pub struct BackupPaths {
    pub blob_dir: PathBuf,
    pub staging: PathBuf,
    /// Files and directories copied into `config/` nightly.
    pub config: Vec<PathBuf>,
}

/// rclone configured entirely through its environment, so no secret is written to disk or argv.
struct Rclone {
    env: Vec<(String, String)>,
}

impl Rclone {
    async fn new(cfg: &BackupConfig) -> Result<Self, String> {
        let base = Self { env: base_env() };
        let obscured = base.run_stdin(&["obscure", "-"], &cfg.key).await?;
        let obscured = obscured.trim().to_string();
        if obscured.is_empty() {
            return Err("rclone obscure returned nothing".into());
        }
        let mut env = base_env();
        let mut set = |k: &str, v: &str| env.push((format!("RCLONE_CONFIG_{k}"), v.to_string()));
        set("R2_TYPE", "s3");
        set("R2_PROVIDER", "Cloudflare");
        set("R2_ENDPOINT", &cfg.endpoint);
        set("R2_ACCESS_KEY_ID", &cfg.access_key_id);
        set("R2_SECRET_ACCESS_KEY", &cfg.secret_access_key);
        set("R2_REGION", "auto");
        set("R2_ACL", "private");
        // An Object Read & Write token cannot check or create buckets (rclone's R2 notes).
        set("R2_NO_CHECK_BUCKET", "true");
        // One crypt remote per prefix, so the prefix stays plaintext and the lock rules match it.
        for (name, dir) in [("BLOBS", "blobs"), ("DB", "db"), ("CONFIG", "config")] {
            set(&format!("{name}_TYPE"), "crypt");
            set(&format!("{name}_REMOTE"), &format!("{}{dir}", cfg.root()));
            set(&format!("{name}_PASSWORD"), &obscured);
        }
        Ok(Self { env })
    }

    async fn run(&self, args: &[&str]) -> Result<String, String> {
        self.exec(args, None).await
    }

    async fn run_stdin(&self, args: &[&str], input: &str) -> Result<String, String> {
        self.exec(args, Some(input)).await
    }

    async fn exec(&self, args: &[&str], input: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("rclone");
        cmd.args(args)
            .env_clear()
            .envs(self.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("rclone {}: could not start: {e}", args[0]))?;
        if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
            stdin
                .write_all(format!("{text}\n").as_bytes())
                .await
                .map_err(|e| format!("rclone {}: stdin: {e}", args[0]))?;
        }
        let out = tokio::time::timeout(Duration::from_secs(1800), child.wait_with_output())
            .await
            .map_err(|_| format!("rclone {} exceeded 30 min and was killed", args[0]))?
            .map_err(|e| format!("rclone {}: {e}", args[0]))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(format!(
                "rclone {} exited {}: {}",
                args[0],
                out.status,
                stderr.trim()
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

/// What rclone may see of the server's environment: only what it needs to run.
fn base_env() -> Vec<(String, String)> {
    ["PATH", "HOME", "TZ"]
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
        .collect()
}

fn is_blob_name(name: &str) -> bool {
    name.len() == 64
        && name
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The hashes among `candidates` that left on purpose: every document holding them was purged
/// and nothing else references them. Only these may leave the backup.
async fn purged(db: &Database, candidates: &[String]) -> Result<BTreeSet<String>, String> {
    if candidates.is_empty() {
        return Ok(BTreeSet::new());
    }
    let mut resp = db
        .query("SELECT VALUE sha256 FROM documents WHERE sha256 IN $hs AND purged = true")
        .bind(("hs", candidates.to_vec()))
        .await
        .map_err(|e| format!("purged blobs query: {e}"))?;
    let hit: Vec<String> = resp
        .take(0)
        .map_err(|e| format!("purged blobs query: {e}"))?;
    let hit: Vec<String> = hit
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let still = queries::blob_reference_counts(db, &hit)
        .await
        .map_err(|e| format!("blob references: {e}"))?;
    Ok(hit
        .into_iter()
        .filter(|h| still.get(h).copied().unwrap_or(0) == 0)
        .collect())
}

#[derive(Debug, Default)]
struct BlobPass {
    counts: BTreeMap<String, u64>,
    /// Blobs missing here that were never purged: lost on the server, still in the backup.
    missing: Vec<String>,
    /// Every blob here that the backup now holds, for the nightly restore check.
    backed_up: Vec<String>,
}

/// The blob names the backup holds. An empty or never-written prefix lists as
/// "directory not found" on object stores, which here means nothing is backed up yet.
async fn list_blobs(rc: &Rclone) -> Result<BTreeSet<String>, String> {
    let listing = match rc.run(&["lsf", "blobs:", "--files-only"]).await {
        Ok(listing) => listing,
        Err(e) if e.contains("directory not found") => String::new(),
        Err(e) => return Err(e),
    };
    Ok(listing
        .lines()
        .map(str::trim)
        .filter(|l| is_blob_name(l))
        .map(String::from)
        .collect())
}

async fn back_up_blobs(
    rc: &Rclone,
    db: &Database,
    paths: &BackupPaths,
) -> Result<BlobPass, String> {
    let mut local = BTreeSet::new();
    let mut dir = tokio::fs::read_dir(&paths.blob_dir)
        .await
        .map_err(|e| format!("read {}: {e}", paths.blob_dir.display()))?;
    while let Some(entry) = dir
        .next_entry()
        .await
        .map_err(|e| format!("read blob dir: {e}"))?
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        // Skips the store's `.tmp-*` files, which are renamed into place once written.
        if is_blob_name(&name) {
            local.insert(name);
        }
    }
    let remote = list_blobs(rc).await?;

    let new: Vec<&String> = local.difference(&remote).collect();
    if !new.is_empty() {
        tokio::fs::create_dir_all(&paths.staging)
            .await
            .map_err(|e| format!("staging dir: {e}"))?;
        let list = paths.staging.join("blobs-to-upload.txt");
        let body: String = new.iter().map(|h| format!("{h}\n")).collect();
        tokio::fs::write(&list, body)
            .await
            .map_err(|e| format!("upload list: {e}"))?;
        let src = paths.blob_dir.to_string_lossy().into_owned();
        let list_arg = list.to_string_lossy().into_owned();
        let args = [
            "copy",
            src.as_str(),
            "blobs:",
            "--files-from-raw",
            list_arg.as_str(),
            "--no-traverse",
        ];
        let result = rc.run(&args).await;
        let _ = tokio::fs::remove_file(&list).await;
        result?;
        // Checked by listing, not by rclone's exit code alone.
        let after = list_blobs(rc).await?;
        let absent = new.iter().filter(|h| !after.contains(h.as_str())).count();
        if absent > 0 {
            return Err(format!(
                "{absent} of {} uploaded blobs are not in the backup",
                new.len()
            ));
        }
    }

    let gone: Vec<String> = remote.difference(&local).cloned().collect();
    let purged = purged(db, &gone).await?;
    let (mut deleted, mut deferred, mut missing) = (0u64, 0u64, Vec::new());
    for h in gone {
        if !purged.contains(&h) {
            missing.push(h);
            continue;
        }
        // Refused while the object is inside its lock period; the next run tries again.
        let target = format!("blobs:{h}");
        match rc.run(&["deletefile", target.as_str()]).await {
            Ok(_) => deleted += 1,
            Err(e) => {
                tracing::info!(blob = %h, error = %e, "backup: purged blob not deleted yet");
                deferred += 1;
            }
        }
    }

    let mut counts = BTreeMap::new();
    counts.insert("local".into(), local.len() as u64);
    counts.insert("in_backup_before".into(), remote.len() as u64);
    counts.insert("uploaded".into(), new.len() as u64);
    counts.insert("deleted_purged".into(), deleted);
    counts.insert("delete_deferred".into(), deferred);
    counts.insert("missing_here".into(), missing.len() as u64);
    Ok(BlobPass {
        counts,
        missing,
        backed_up: local.into_iter().collect(),
    })
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn gzip(src: &Path, dst: &Path) -> Result<u64, String> {
    use flate2::{Compression, write::GzEncoder};
    let mut input = std::fs::File::open(src).map_err(|e| format!("{}: {e}", src.display()))?;
    let out = std::fs::File::create(dst).map_err(|e| format!("{}: {e}", dst.display()))?;
    let mut enc = GzEncoder::new(out, Compression::default());
    std::io::copy(&mut input, &mut enc).map_err(|e| format!("gzip: {e}"))?;
    enc.finish().map_err(|e| format!("gzip: {e}"))?;
    std::fs::metadata(dst)
        .map(|m| m.len())
        .map_err(|e| format!("gzip: {e}"))
}

/// Copy a file, or a directory tree, under `dst`. A missing source is skipped and reported.
fn copy_into(src: &Path, dst: &Path) -> Result<u64, String> {
    if !src.exists() {
        tracing::warn!(path = %src.display(), "backup: config path missing; skipped");
        return Ok(0);
    }
    let name = src
        .file_name()
        .ok_or_else(|| format!("{} has no name", src.display()))?;
    let target = dst.join(name);
    if src.is_file() {
        std::fs::copy(src, &target).map_err(|e| format!("{}: {e}", src.display()))?;
        return Ok(1);
    }
    std::fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
    let mut n = 0;
    for entry in std::fs::read_dir(src).map_err(|e| format!("{}: {e}", src.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", src.display()))?;
        n += copy_into(&entry.path(), &target)?;
    }
    Ok(n)
}

/// The deploy settings as the process sees them; the deploy files themselves are not mounted here.
fn env_dump() -> String {
    let mut vars: Vec<(String, String)> = std::env::vars()
        .filter(|(k, _)| k.starts_with("OMNI_"))
        .collect();
    vars.sort();
    vars.into_iter()
        .map(|(k, v)| format!("{k}={v}\n"))
        .collect()
}

#[derive(Debug, Default)]
struct NightlyPass {
    counts: BTreeMap<String, u64>,
    over_size: Option<u64>,
}

async fn nightly(
    rc: &Rclone,
    cfg: &BackupConfig,
    db: &Database,
    paths: &BackupPaths,
    sample_from: &[String],
    last_export: Option<u64>,
) -> Result<NightlyPass, String> {
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let dir = paths.staging.join(&stamp);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("staging {}: {e}", dir.display()))?;
    let result = nightly_in(rc, cfg, db, paths, sample_from, last_export, &stamp, &dir).await;
    let _ = tokio::fs::remove_dir_all(&dir).await;
    result
}

#[allow(clippy::too_many_arguments)]
async fn nightly_in(
    rc: &Rclone,
    cfg: &BackupConfig,
    db: &Database,
    paths: &BackupPaths,
    sample_from: &[String],
    last_export: Option<u64>,
    stamp: &str,
    dir: &Path,
) -> Result<NightlyPass, String> {
    let mut counts = BTreeMap::new();

    // The database, checked by size and not by the export's success alone.
    let raw = dir.join("db.surql");
    db.export(&raw)
        .await
        .map_err(|e| format!("database export: {e}"))?;
    let raw_bytes = tokio::fs::metadata(&raw)
        .await
        .map_err(|e| format!("database export unreadable: {e}"))?
        .len();
    if raw_bytes == 0 {
        return Err("database export is empty".into());
    }
    if let Some(prev) = last_export
        && (raw_bytes as f64) < prev as f64 * MIN_EXPORT_RATIO
    {
        return Err(format!(
            "database export is {raw_bytes} bytes, under half the previous {prev}; not uploaded"
        ));
    }
    let gz = dir.join(format!("{stamp}.surql.gz"));
    let (raw2, gz2) = (raw.clone(), gz.clone());
    let gz_bytes = tokio::task::spawn_blocking(move || gzip(&raw2, &gz2))
        .await
        .map_err(|e| format!("gzip task: {e}"))??;
    let _ = tokio::fs::remove_file(&raw).await;
    let gz_arg = gz.to_string_lossy().into_owned();
    let db_dst = format!("db:{stamp}.surql.gz");
    rc.run(&["copyto", gz_arg.as_str(), db_dst.as_str()])
        .await?;
    // Round trip: the export must come back through the crypt layer byte for byte.
    let back = dir.join("db-roundtrip.gz");
    let back_arg = back.to_string_lossy().into_owned();
    rc.run(&["copyto", db_dst.as_str(), back_arg.as_str()])
        .await?;
    let (a, b) = (gz.clone(), back.clone());
    let same =
        tokio::task::spawn_blocking(move || Ok::<_, String>(sha256_file(&a)? == sha256_file(&b)?))
            .await
            .map_err(|e| format!("hash task: {e}"))??;
    if !same {
        return Err("database export did not round-trip through the backup unchanged".into());
    }
    counts.insert("db_export_bytes".into(), raw_bytes);
    counts.insert("db_export_gz_bytes".into(), gz_bytes);

    // Config: credentials, the app's state files, and the deploy settings.
    let cfg_dir = dir.join("config");
    tokio::fs::create_dir_all(&cfg_dir)
        .await
        .map_err(|e| format!("config staging: {e}"))?;
    let (sources, target) = (paths.config.clone(), cfg_dir.clone());
    let files = tokio::task::spawn_blocking(move || {
        let mut n = 0;
        for src in &sources {
            n += copy_into(src, &target)?;
        }
        Ok::<_, String>(n)
    })
    .await
    .map_err(|e| format!("config copy task: {e}"))??;
    tokio::fs::write(cfg_dir.join("omni-env.txt"), env_dump())
        .await
        .map_err(|e| format!("env dump: {e}"))?;
    let (cfg_src, cfg_dst) = (
        cfg_dir.to_string_lossy().into_owned(),
        format!("config:{stamp}"),
    );
    rc.run(&["copy", cfg_src.as_str(), cfg_dst.as_str()])
        .await?;
    counts.insert("config_files".into(), files + 1);

    // Pruning runs only after tonight's copies are safely up.
    rc.run(&["delete", "db:", "--min-age", KEEP]).await?;
    rc.run(&["delete", "config:", "--min-age", KEEP, "--rmdirs"])
        .await?;

    // Restore check: random blobs must come back hashing to their own names.
    let restore = dir.join("restore");
    tokio::fs::create_dir_all(&restore)
        .await
        .map_err(|e| format!("restore staging: {e}"))?;
    let picks: Vec<String> = sample_from
        .choose_multiple(&mut rand::thread_rng(), RESTORE_SAMPLE)
        .cloned()
        .collect();
    for h in &picks {
        let out = restore.join(h);
        let (from, to) = (format!("blobs:{h}"), out.to_string_lossy().into_owned());
        rc.run(&["copyto", from.as_str(), to.as_str()]).await?;
        let got = tokio::task::spawn_blocking(move || sha256_file(&out))
            .await
            .map_err(|e| format!("hash task: {e}"))??;
        if &got != h {
            return Err(format!("restored blob {h} hashes to {got}"));
        }
    }
    counts.insert("restore_checked".into(), picks.len() as u64);

    // Bucket size, against the free tier. A missing field fails loudly rather than reading as 0.
    let root = cfg.root();
    let size = rc.run(&["size", root.as_str(), "--json"]).await?;
    let bytes = serde_json::from_str::<serde_json::Value>(&size)
        .ok()
        .and_then(|v| v.get("bytes").and_then(|b| b.as_u64()))
        .ok_or_else(|| format!("rclone size --json had no bytes field: {}", size.trim()))?;
    counts.insert("bucket_bytes".into(), bytes);
    Ok(NightlyPass {
        counts,
        over_size: (bytes > WARN_BYTES).then_some(bytes),
    })
}

/// Runs the backup until the process ends. Each tick backs up blobs; the first tick of each UTC
/// day with no successful nightly run also does the nightly pass.
pub struct BackupJob {
    pub cfg: BackupConfig,
    pub db: Database,
    pub paths: BackupPaths,
    pub runs: Arc<dyn RunStore>,
    pub observers: Vec<Arc<dyn RunObserver>>,
}

impl BackupJob {
    pub fn spawn(self) {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(TICK);
            loop {
                tick.tick().await;
                self.tick().await;
            }
        });
    }

    async fn record(&self, run: RunRecord) {
        if let Err(e) = self.runs.record(&run).await {
            tracing::error!(error = %e, "backup: run not recorded");
        }
        for o in &self.observers {
            o.observe(&run).await;
        }
    }

    async fn tick(&self) {
        let started = Utc::now();
        let rc = match Rclone::new(&self.cfg).await {
            Ok(rc) => rc,
            Err(e) => {
                tracing::error!(error = %e, "backup: rclone unavailable");
                let outcome = RunOutcome::Failure {
                    error: bounded_detail(&e),
                };
                self.record(RunRecord::new(
                    JOB,
                    SUBJECT_BLOBS,
                    started,
                    Utc::now(),
                    outcome,
                ))
                .await;
                return;
            }
        };

        let blobs = back_up_blobs(&rc, &self.db, &self.paths).await;
        let sample = blobs
            .as_ref()
            .map(|b| b.backed_up.clone())
            .unwrap_or_default();
        let (outcome, counts) = match blobs {
            Ok(pass) if !pass.missing.is_empty() => {
                let reason = format!(
                    "{} blob(s) are in the backup but missing on the server and were never purged; \
                     restore them, e.g. {}",
                    pass.missing.len(),
                    pass.missing[0]
                );
                tracing::error!(
                    missing = pass.missing.len(),
                    "backup: blobs lost on the server"
                );
                (RunOutcome::NeedsUser { reason }, pass.counts)
            }
            Ok(pass) => (RunOutcome::Success, pass.counts),
            Err(e) => {
                tracing::error!(error = %e, "backup: blob pass failed");
                (
                    RunOutcome::Failure {
                        error: bounded_detail(&e),
                    },
                    BTreeMap::new(),
                )
            }
        };
        tracing::info!(?counts, "backup: blob pass");
        let mut run = RunRecord::new(JOB, SUBJECT_BLOBS, started, Utc::now(), outcome);
        run.counts = counts;
        self.record(run).await;

        let last = match self.runs.last_success(JOB, SUBJECT_NIGHTLY).await {
            Ok(last) => last,
            Err(e) => {
                tracing::warn!(error = %e, "backup: nightly history unreadable; running it");
                None
            }
        };
        if last
            .as_ref()
            .is_some_and(|r| r.ended_at.date_naive() == Utc::now().date_naive())
        {
            return;
        }
        let last_export = last.and_then(|r| r.counts.get("db_export_bytes").copied());
        let started = Utc::now();
        let (outcome, counts) =
            match nightly(&rc, &self.cfg, &self.db, &self.paths, &sample, last_export).await {
                Ok(pass) => match pass.over_size {
                    Some(bytes) => (
                        RunOutcome::NeedsUser {
                            reason: format!(
                                "the backup bucket holds {:.1} GB, near the free 10 GB",
                                bytes as f64 / 1e9
                            ),
                        },
                        pass.counts,
                    ),
                    None => (RunOutcome::Success, pass.counts),
                },
                Err(e) => {
                    tracing::error!(error = %e, "backup: nightly pass failed");
                    (
                        RunOutcome::Failure {
                            error: bounded_detail(&e),
                        },
                        BTreeMap::new(),
                    )
                }
            };
        tracing::info!(?counts, "backup: nightly pass");
        let mut run = RunRecord::new(JOB, SUBJECT_NIGHTLY, started, Utc::now(), outcome);
        run.counts = counts;
        self.record(run).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secrets(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn full() -> Vec<(&'static str, &'static str)> {
        vec![
            ("backup_endpoint", "https://acct.r2.cloudflarestorage.com"),
            ("backup_access_key_id", "id"),
            ("backup_secret_access_key", "secret"),
            ("backup_bucket", "bucket"),
            ("backup_key", "key"),
        ]
    }

    #[test]
    fn no_backup_secrets_means_off() {
        assert!(
            BackupConfig::from_secrets(&HashMap::new())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn some_backup_secrets_name_the_missing_ones() {
        let err = BackupConfig::from_secrets(&secrets(&full()[..2]))
            .err()
            .unwrap();
        assert!(
            err.contains("backup_bucket") && err.contains("backup_key"),
            "{err}"
        );
    }

    #[test]
    fn a_plain_http_endpoint_is_refused() {
        let mut s = full();
        s[0] = ("backup_endpoint", "http://acct.r2.cloudflarestorage.com");
        assert!(BackupConfig::from_secrets(&secrets(&s)).is_err());
    }

    #[test]
    fn the_prefix_must_be_a_folder() {
        let mut s = full();
        s.push(("backup_prefix", "dev"));
        assert!(BackupConfig::from_secrets(&secrets(&s)).is_err());
        s.pop();
        s.push(("backup_prefix", "dev/"));
        let cfg = BackupConfig::from_secrets(&secrets(&s)).unwrap().unwrap();
        assert_eq!(cfg.root(), "r2:bucket/dev/");
    }

    #[test]
    fn only_full_hashes_count_as_blobs() {
        let h = "0039f76ba72ff2adea6617571e42b11b418340b80cce378883699c94ddb079de";
        assert!(is_blob_name(h));
        assert!(!is_blob_name(".tmp-01M821MAF3GYZW1DZM6HJHWMRJ"));
        assert!(!is_blob_name(&h.to_uppercase()));
        assert!(!is_blob_name(&h[1..]));
    }

    #[test]
    fn gzip_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let (src, dst) = (dir.path().join("a"), dir.path().join("a.gz"));
        std::fs::write(&src, "x".repeat(10_000)).unwrap();
        let n = gzip(&src, &dst).unwrap();
        assert!(n > 0 && n < 10_000);
        let mut out = String::new();
        use std::io::Read;
        flate2::read::GzDecoder::new(std::fs::File::open(&dst).unwrap())
            .read_to_string(&mut out)
            .unwrap();
        assert_eq!(out.len(), 10_000);
    }

    #[test]
    fn config_copy_keeps_the_tree_and_skips_what_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("state");
        std::fs::create_dir_all(src.join("omni-me")).unwrap();
        std::fs::write(src.join("omni-me/paused_sources.toml"), "x").unwrap();
        let dst = dir.path().join("out");
        std::fs::create_dir_all(&dst).unwrap();
        assert_eq!(copy_into(&src, &dst).unwrap(), 1);
        assert!(dst.join("state/omni-me/paused_sources.toml").exists());
        assert_eq!(copy_into(&dir.path().join("absent"), &dst).unwrap(), 0);
    }
}
