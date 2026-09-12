//! Content-addressed blob storage — one directory, files named by sha256.
//!
//! What content-addressing does and does not buy here: `docs/src/archive.md`.
//!
//! ⚠️ **Identical bytes are one blob, never one document.** A statement that
//! arrives by email and the same statement scanned from paper are one file here
//! and two archive entries, which is why `DocumentArchivedPayload` keys on its
//! own `document_id` rather than on this hash.
//!
//! ⛔ **Blobs are not synced.** They live on whichever host wrote them; devices
//! keep a bounded LRU cache and re-fetch on miss. Anything that must be
//! searchable on a device that never opened the file has to travel in an event
//! instead — see `DocumentArchivedPayload::text`.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    #[error("blob io ({context}): {source}")]
    Io {
        context: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("blob hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("not a sha256 hash: must be 64 lowercase hex characters")]
    InvalidHash,
}

fn io(context: &'static str) -> impl FnOnce(std::io::Error) -> BlobError {
    move |source| BlobError::Io { context, source }
}

/// Lowercase hex sha256 of `bytes` — the name a blob is stored under.
pub fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Reject anything that is not a lowercase 64-char hex string.
///
/// ⚠️ A path guard, not a formatting nicety: the hash is joined onto `dir` to
/// build a filename, so a value carrying `/` or `..` would address a file
/// outside the blob directory entirely.
pub fn validate_hash(candidate: &str) -> Result<(), BlobError> {
    if candidate.len() != 64
        || !candidate
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    {
        return Err(BlobError::InvalidHash);
    }
    Ok(())
}

/// Where a hash lives under `dir`. Validates before joining.
pub fn path_for(dir: &Path, hash: &str) -> Result<PathBuf, BlobError> {
    validate_hash(hash)?;
    Ok(dir.join(hash))
}

/// Store `bytes` under their own hash, returning it. Idempotent.
///
/// ⚠️ **Write to a temp name, then rename.** A reader can open a blob the moment
/// its name exists, so writing in place publishes a half-written file under a
/// name that promises its content. The rename is atomic within the directory, so
/// a blob is either absent or complete — never truncated but correctly named.
pub async fn store(dir: &Path, bytes: &[u8]) -> Result<String, BlobError> {
    let digest = hash(bytes);
    store_as(dir, &digest, bytes).await?;
    Ok(digest)
}

/// Store `bytes` under a hash the caller already has, verifying they match.
///
/// The upload path's shape: the client names the hash in the URL, so the bytes
/// have to be checked against it rather than trusted. ⛔ Never skip the check on
/// the grounds that the caller just computed it — the whole point of the route
/// is that the caller is remote.
pub async fn store_as(dir: &Path, expected: &str, bytes: &[u8]) -> Result<bool, BlobError> {
    validate_hash(expected)?;
    let final_path = dir.join(expected);

    // Checked before hashing, so a re-upload of a large blob costs a stat rather
    // than a full digest.
    if tokio::fs::try_exists(&final_path)
        .await
        .map_err(io("exists check"))?
    {
        return Ok(false);
    }

    let actual = hash(bytes);
    if actual != expected {
        return Err(BlobError::HashMismatch {
            expected: expected.to_string(),
            actual,
        });
    }

    let tmp = dir.join(format!(".tmp-{}", ulid::Ulid::new()));
    tokio::fs::write(&tmp, bytes)
        .await
        .map_err(io("temp write"))?;
    tokio::fs::rename(&tmp, &final_path)
        .await
        .map_err(io("rename"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[tokio::test]
    async fn storing_the_same_bytes_twice_is_a_no_op() {
        let d = dir();
        let first = store(d.path(), b"a statement").await.unwrap();
        let second = store(d.path(), b"a statement").await.unwrap();
        assert_eq!(first, second);

        let files: Vec<_> = std::fs::read_dir(d.path()).unwrap().flatten().collect();
        assert_eq!(files.len(), 1, "one blob, not two");
    }

    #[tokio::test]
    async fn bytes_that_do_not_match_their_claimed_hash_are_refused() {
        let d = dir();
        let wrong = "b".repeat(64);
        let err = store_as(d.path(), &wrong, b"a statement")
            .await
            .unwrap_err();
        assert!(matches!(err, BlobError::HashMismatch { .. }));
        assert!(
            !d.path().join(&wrong).exists(),
            "a refused upload must leave nothing behind"
        );
    }

    #[tokio::test]
    async fn a_traversing_hash_cannot_address_a_file_outside_the_dir() {
        let d = dir();
        for bad in ["../etc/passwd", "..", "a/b", &"A".repeat(64), "short"] {
            assert!(
                matches!(validate_hash(bad), Err(BlobError::InvalidHash)),
                "{bad} must be refused"
            );
            assert!(path_for(d.path(), bad).is_err());
        }
    }

    #[tokio::test]
    async fn a_temp_file_is_never_left_behind_on_success() {
        let d = dir();
        store(d.path(), b"x").await.unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(d.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(".tmp-"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[tokio::test]
    async fn store_reports_whether_it_wrote_anything() {
        let d = dir();
        let h = hash(b"once");
        assert!(store_as(d.path(), &h, b"once").await.unwrap());
        assert!(
            !store_as(d.path(), &h, b"once").await.unwrap(),
            "the second call must report that the blob was already there"
        );
    }
}
