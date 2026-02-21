//! On-disk cache for per-function git touch metrics.
//!
//! Cache key: `"{sha}:{file}:{start}:{end}"` where file is a repo-relative path
//! and start/end are 1-based line numbers. Value: `(touch_count_30d, days_since_last_change)`.
//!
//! **Line range shift behavior:** If surrounding code changes and a function's line
//! range moves, the cache key will not match (start/end differ) — it is a miss.
//! This is correct: the function's git history in its new position must be re-queried.
//! An unchanged function in an unchanged file hits the parent commit's cache entry
//! exactly. Do not "fix" this with content hashing — the miss behavior is intentional.
//!
//! **Eviction:** After writing, entries whose SHA prefix is not in the provided set
//! are dropped. At most `MAX_CACHED_SHAS` distinct SHAs are retained to bound file size.
//!
//! **Warm-run speedup (SQ-1d benchmark, hyperfine, this repo, ~200 functions):**
//!   cold run            6.2 s ± 0.02 s  (one `git log -L` subprocess per function)
//!   warm run          230 ms ± 3 ms     (all cache hits — no subprocesses)
//!   file-level        268 ms ± 3 ms     (batched git log per file)
//! Warm is ~27× faster than cold and ~15% faster than file-level. Target was warm ≤ 2×
//! file-level; this repo achieves 0.86× (warm per-function beats file-level).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// In-memory touch cache: maps key to `(touch_count_30d, days_since_last_change)`.
pub type TouchCache = HashMap<String, (usize, Option<u32>)>;

/// Maximum number of distinct commit SHAs to retain in the cache.
const MAX_CACHED_SHAS: usize = 50;

fn cache_path(repo_root: &Path) -> PathBuf {
    crate::snapshot::hotspots_dir(repo_root).join("touch-cache.json.zst")
}

/// Build a cache lookup key from its components.
pub fn cache_key(sha: &str, file: &str, start: u32, end: u32) -> String {
    format!("{}:{}:{}:{}", sha, file, start, end)
}

/// Load the touch cache from disk.
///
/// Returns `None` on cold start (file absent) or on read/decompress error (non-fatal).
/// The caller should treat `None` as an empty cache and proceed normally.
pub fn read_touch_cache(repo_root: &Path) -> Option<TouchCache> {
    let path = cache_path(repo_root);
    if !path.exists() {
        return None;
    }
    match load_compressed_json(&path) {
        Ok(cache) => Some(cache),
        Err(e) => {
            eprintln!("warning: failed to load touch cache (proceeding cold): {e}");
            None
        }
    }
}

fn load_compressed_json(path: &Path) -> Result<TouchCache> {
    let compressed = std::fs::read(path)
        .with_context(|| format!("failed to read touch cache: {}", path.display()))?;
    let bytes = zstd::decode_all(compressed.as_slice())
        .with_context(|| format!("failed to decompress touch cache: {}", path.display()))?;
    let json = std::str::from_utf8(&bytes).context("touch cache is not valid UTF-8")?;
    serde_json::from_str(json).context("failed to parse touch cache JSON")
}

/// Write the touch cache to disk (zstd level 3).
pub fn write_touch_cache(repo_root: &Path, cache: &TouchCache) -> Result<()> {
    let path = cache_path(repo_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }
    let json = serde_json::to_string(cache).context("failed to serialize touch cache")?;
    let compressed =
        zstd::encode_all(json.as_bytes(), 3).context("failed to compress touch cache")?;
    std::fs::write(&path, &compressed)
        .with_context(|| format!("failed to write touch cache: {}", path.display()))
}

/// Evict cache entries whose SHA is not among `known_shas`.
///
/// `known_shas` should be ordered most-recent-first; at most `MAX_CACHED_SHAS` are
/// retained. This bounds file size on repositories with many historic commits.
pub fn evict_old_entries(cache: &mut TouchCache, known_shas: &[String]) {
    let allowed: std::collections::HashSet<&str> = known_shas
        .iter()
        .take(MAX_CACHED_SHAS)
        .map(String::as_str)
        .collect();
    // Key format: "{sha}:{file}:{start}:{end}" — SHA is the segment before the first ':'
    cache.retain(|key, _| {
        key.split(':')
            .next()
            .is_some_and(|sha| allowed.contains(sha))
    });
}
