//! Local attachment cache (Phase 3.7).
//!
//! On-device LRU mirror of the server-side `/blobs/{sha256}` store. Capture
//! flows write through here so a captured receipt stays viewable offline; the
//! Phase 4 transaction detail view will fetch through here, falling back to a
//! `GET /blobs/{sha256}` and re-populating the cache on miss.
//!
//! Eviction policy: when `cache_size > CAP_BYTES`, sort entries by mtime
//! ascending and delete until under the cap. `atime` is unreliable on
//! `noatime`-mounted filesystems (Linux default), so cache hits explicitly
//! `touch` mtime to keep frequently-accessed blobs in residency.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::State;

use crate::AppState;

/// 200 MB on-device cap per [[project-auto-import-strategy]] / Phase 3.7 plan.
const CAP_BYTES: u64 = 200 * 1024 * 1024;

/// Lowercase hex-sha256 character set, matches the server validator at
/// `server/src/routes/blobs.rs::validate_hash_format`.
fn validate_hash(hash: &str) -> Result<(), String> {
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()) {
        return Err("hash must be 64 lowercase hex chars".into());
    }
    Ok(())
}

/// Write `bytes` to the local cache as `<dir>/<sha256>`. Idempotent — if the
/// file already exists we just `touch` it so its mtime moves forward (cache
/// hit promotion). After write, enforce the LRU cap.
pub async fn cache_write(dir: &Path, sha256: &str, bytes: &[u8]) -> Result<(), String> {
    validate_hash(sha256)?;
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| format!("cache dir create: {e}"))?;

    let path = dir.join(sha256);
    if tokio::fs::try_exists(&path)
        .await
        .map_err(|e| format!("cache exists check: {e}"))?
    {
        touch(&path).await.ok();
    } else {
        let tmp = dir.join(format!(".tmp-{}", ulid::Ulid::new()));
        tokio::fs::write(&tmp, bytes)
            .await
            .map_err(|e| format!("cache temp write: {e}"))?;
        tokio::fs::rename(&tmp, &path)
            .await
            .map_err(|e| format!("cache rename: {e}"))?;
    }

    evict_to_cap(dir, CAP_BYTES).await.ok();
    Ok(())
}

/// Read `<dir>/<sha256>`. Returns `Ok(None)` on miss (caller should fall back
/// to `GET /blobs/{sha256}`). On hit, mtime is bumped so the entry rises in
/// the LRU.
async fn cache_read(dir: &Path, sha256: &str) -> Result<Option<Vec<u8>>, String> {
    validate_hash(sha256)?;
    let path = dir.join(sha256);
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            touch(&path).await.ok();
            Ok(Some(bytes))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cache read: {e}")),
    }
}

/// Set mtime to now without changing contents. Uses `filetime` semantics via
/// `std::fs::File::set_modified` (available since Rust 1.75).
async fn touch(path: &Path) -> Result<(), String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let file = std::fs::OpenOptions::new().write(true).open(&path)?;
        file.set_modified(SystemTime::now())?;
        Ok(())
    })
    .await
    .map_err(|e| format!("touch join: {e}"))?
    .map_err(|e| format!("touch fs: {e}"))
}

/// Return total cache size in bytes (sum of regular file sizes under `dir`).
/// Used by the upcoming Settings → Cache panel (3.8) and the eviction loop.
pub async fn cache_size(dir: &Path) -> Result<u64, String> {
    if !tokio::fs::try_exists(dir)
        .await
        .map_err(|e| format!("cache exists check: {e}"))?
    {
        return Ok(0);
    }
    let mut total = 0u64;
    let mut rd = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| format!("cache readdir: {e}"))?;
    while let Some(entry) = rd
        .next_entry()
        .await
        .map_err(|e| format!("cache entry: {e}"))?
    {
        let md = entry
            .metadata()
            .await
            .map_err(|e| format!("cache metadata: {e}"))?;
        if md.is_file() {
            total += md.len();
        }
    }
    Ok(total)
}

/// If `cache_size > cap`, delete oldest-mtime files until back under cap.
/// Skips `.tmp-*` files (in-flight writes) so a concurrent put isn't yanked.
async fn evict_to_cap(dir: &Path, cap: u64) -> Result<(), String> {
    let mut entries: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
    let mut rd = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| format!("evict readdir: {e}"))?;
    while let Some(entry) = rd
        .next_entry()
        .await
        .map_err(|e| format!("evict entry: {e}"))?
    {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with(".tmp-") {
            continue;
        }
        let md = match entry.metadata().await {
            Ok(m) if m.is_file() => m,
            _ => continue,
        };
        let mtime = md.modified().unwrap_or(UNIX_EPOCH);
        entries.push((path, md.len(), mtime));
    }

    let mut total: u64 = entries.iter().map(|(_, sz, _)| *sz).sum();
    if total <= cap {
        return Ok(());
    }
    entries.sort_by_key(|(_, _, mtime)| *mtime);
    for (path, sz, _) in entries {
        if total <= cap {
            break;
        }
        if tokio::fs::remove_file(&path).await.is_ok() {
            total = total.saturating_sub(sz);
        }
    }
    Ok(())
}

/// Best-effort directory wipe — used by Settings → Clear Attachment Cache
/// (Phase 3.8). Removes regular files only; leaves the directory itself in
/// place so future writes don't have to recreate it.
pub async fn cache_clear(dir: &Path) -> Result<u64, String> {
    if !tokio::fs::try_exists(dir)
        .await
        .map_err(|e| format!("cache exists check: {e}"))?
    {
        return Ok(0);
    }
    let mut removed = 0u64;
    let mut rd = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| format!("cache readdir: {e}"))?;
    while let Some(entry) = rd
        .next_entry()
        .await
        .map_err(|e| format!("cache entry: {e}"))?
    {
        let path = entry.path();
        if let Ok(md) = entry.metadata().await
            && md.is_file()
            && tokio::fs::remove_file(&path).await.is_ok()
        {
            removed += md.len();
        }
    }
    Ok(removed)
}

// --- Tauri commands ---

/// Return raw bytes for a stored attachment. Cache-first; on miss, fetch from
/// `GET /blobs/{sha256}` and populate the cache before returning.
#[tauri::command(rename_all = "snake_case")]
pub async fn fetch_attachment(
    state: State<'_, AppState>,
    sha256: String,
) -> Result<Vec<u8>, String> {
    if let Some(bytes) = cache_read(&state.attachment_cache_dir, &sha256).await? {
        return Ok(bytes);
    }

    let server_url = state.server_url.read().await.clone();
    let url = format!("{}/blobs/{sha256}", server_url.trim_end_matches('/'));
    let resp = state
        .http
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("attachment fetch: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("attachment fetch: server returned {}", resp.status()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("attachment fetch body: {e}"))?
        .to_vec();
    cache_write(&state.attachment_cache_dir, &sha256, &bytes).await?;
    Ok(bytes)
}

/// Settings → Cache: current cache size in bytes (3.8 surface).
#[tauri::command(rename_all = "snake_case")]
pub async fn attachment_cache_size(state: State<'_, AppState>) -> Result<u64, String> {
    cache_size(&state.attachment_cache_dir).await
}

/// Settings → Cache: clear all cached attachments; returns bytes freed.
#[tauri::command(rename_all = "snake_case")]
pub async fn clear_attachment_cache(state: State<'_, AppState>) -> Result<u64, String> {
    cache_clear(&state.attachment_cache_dir).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_then_read_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let sha = "0".repeat(64);
        cache_write(tmp.path(), &sha, b"hello").await.unwrap();
        let got = cache_read(tmp.path(), &sha).await.unwrap();
        assert_eq!(got.as_deref(), Some(&b"hello"[..]));
    }

    #[tokio::test]
    async fn miss_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let sha = "a".repeat(64);
        let got = cache_read(tmp.path(), &sha).await.unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn evict_drops_oldest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        for i in 0..4 {
            let sha = format!("{i:064x}");
            let bytes = vec![0u8; 60];
            cache_write(dir, &sha, &bytes).await.unwrap();
            // Tiny gap so mtime differs reliably across filesystems.
            tokio::time::sleep(std::time::Duration::from_millis(15)).await;
        }
        evict_to_cap(dir, 120).await.unwrap();
        let size = cache_size(dir).await.unwrap();
        assert!(size <= 120, "expected <=120 after eviction, got {size}");
        // Oldest two entries should be gone, newest two retained.
        let mut survivors: Vec<String> = Vec::new();
        let mut rd = tokio::fs::read_dir(dir).await.unwrap();
        while let Some(entry) = rd.next_entry().await.unwrap() {
            survivors.push(entry.file_name().to_string_lossy().to_string());
        }
        assert!(survivors.contains(&format!("{:064x}", 3)));
        assert!(survivors.contains(&format!("{:064x}", 2)));
        assert!(!survivors.contains(&format!("{:064x}", 0)));
    }

    #[tokio::test]
    async fn invalid_hash_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(cache_write(tmp.path(), "short", b"x").await.is_err());
        assert!(cache_write(tmp.path(), &"G".repeat(64), b"x").await.is_err());
    }

    #[tokio::test]
    async fn clear_removes_files_keeps_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        cache_write(dir, &"0".repeat(64), b"abc").await.unwrap();
        cache_write(dir, &"1".repeat(64), b"def").await.unwrap();
        let freed = cache_clear(dir).await.unwrap();
        assert_eq!(freed, 6);
        assert_eq!(cache_size(dir).await.unwrap(), 0);
        assert!(dir.exists());
    }
}
