//! On-disk cache for per-function [`crate::metrics::RawMetrics`], keyed by file
//! content hash rather than git SHA.
//!
//! Unlike [`crate::touch_cache`] (which caches *git history* metrics and is
//! correctly keyed by commit SHA, since touch counts depend on git position),
//! `RawMetrics` (`cc`, `nd`, `fo`, `ns`, `loc`) depend **only on file content** —
//! parsing the same bytes always yields the same metrics, regardless of which
//! commit, branch, or even repo they live in. So the cache key here is a content
//! hash (BLAKE3, chosen for speed over SHA-2 — we don't need cryptographic
//! collision resistance, just a fast, well-distributed digest for a cache key),
//! not a SHA. There is deliberately no extra invalidation logic beyond the hash
//! lookup itself: same content -> same hash -> same metrics, deterministically.
//!
//! **Cache key:** `"v{CACHE_VERSION}:{content_hash}:{start_line}:{end_line}"`.
//! The hash is of the *whole file's* source text (not a per-function slice):
//! extracting an exact per-function byte range is straightforward for
//! ECMAScript (swc spans) but each of the other five language backends
//! (tree-sitter Go/Java/Python/C/C#, syn Rust) represents spans differently,
//! and getting that extraction subtly wrong per-language would silently
//! produce stale hits. Hashing the whole file is simpler and strictly safe:
//! the tradeoff is that any change anywhere in a file invalidates every
//! function in that file, not just the one that changed.
//!
//! **Per-entry versioning:** `CACHE_VERSION` is embedded directly in each
//! entry's key, not just checked once at file-load time. This means adding a
//! new field to `RawMetrics` later and bumping `CACHE_VERSION` does not force
//! a blanket wipe of the cache file — old-versioned keys simply never match a
//! newly-computed lookup key again, so they miss cleanly (and get dropped on
//! the next [`evict_old_entries`] pass) instead of deserializing into stale
//! data.
//!
//! **Eviction:** after writing, entries whose content hash is not in the set
//! of hashes seen during the current run are dropped, and the cache is capped
//! at `MAX_CACHED_HASHES` entries to bound file size (same style as
//! [`crate::touch_cache`]'s `MAX_CACHED_SHAS`).
//!
//! **Skipping CFG construction on a hit:** callers check [`peek`] *before*
//! building the CFG at all (see `analysis::analyze_function`), and only call
//! [`insert`] after a cache miss has gone through CFG construction and
//! `extract_metrics`. So a cache hit skips both CFG construction and metric
//! extraction, not just the metrics step.
//!
//! **Measured impact (hyperfine, facebook/react clone, `d5736f098e`, 4,046
//! functions across 2,024 non-skipped files, full `analyze` run via
//! `--mode snapshot --format json --all-functions --no-persist`):**
//!
//! Full pipeline, both `touch-cache.json.zst` and `metrics-cache.json.zst`
//! empty vs. both fully populated (1 run cold — too slow for repeated
//! sampling — vs. 8-run hyperfine mean warm):
//!   cold (no caches at all)            7.1 s (single run)
//!   warm (both caches populated)      2.378 s ± 0.013 s  (8 runs)
//! That ~3x looks like a win, but it is almost entirely
//! [`crate::touch_cache`]'s existing effect (see its own doc comment: ~27x on
//! git-log subprocess elimination) — not this new cache.
//!
//! Isolating *this* cache's contribution — `touch-cache.json.zst` kept warm
//! in both cases, `metrics-cache.json.zst` deleted before every cold run vs.
//! left warm across every run (8-run hyperfine means):
//!   cold (metrics cache empty each run)   2.365 s ± 0.009 s
//!   warm (metrics cache populated)        2.378 s ± 0.013 s
//! These are statistically indistinguishable (warm is not faster — within
//! noise of cold). Reported plainly rather than oversold: at this repo's
//! scale, whole-file **parsing** (swc/tree-sitter), which this cache cannot
//! skip — it has to happen before a function's line span is even known, so
//! the cache can be consulted — dominates wall-clock time, not CFG
//! construction + `extract_metrics`, which is the only work this cache
//! actually elides. This mirrors a prior finding on this same repo (merging
//! 4 AST visitor passes into 1 saved 0ms, because decision-point counting was
//! never the bottleneck either): the win from caching `RawMetrics` alone,
//! without also caching parse output or function discovery, is not
//! measurable here. A cache that could skip parsing entirely (e.g. one keyed
//! on content hash that also stores function spans/names, not just
//! `RawMetrics`) would be a different, larger change than what this module
//! does.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result};

use crate::metrics::RawMetrics;

/// Bumped whenever `RawMetrics`'s shape or derivation changes in a way that
/// would make previously-cached entries wrong rather than merely stale.
const CACHE_VERSION: u32 = 1;

/// Maximum number of distinct cache entries to retain, bounding file size.
const MAX_CACHED_HASHES: usize = 20_000;

/// In-memory metrics cache: maps `cache_key(...)` to its cached `RawMetrics`.
pub type MetricsCache = HashMap<String, RawMetrics>;

fn cache_path(repo_root: &Path) -> PathBuf {
    crate::snapshot::hotspots_dir(repo_root).join("metrics-cache.json.zst")
}

/// Hash a file's full source text for use as a cache key component.
pub fn hash_content(src: &str) -> String {
    blake3::hash(src.as_bytes()).to_hex().to_string()
}

/// Build a cache lookup key from its components.
pub fn cache_key(content_hash: &str, start_line: u32, end_line: u32) -> String {
    format!("v{CACHE_VERSION}:{content_hash}:{start_line}:{end_line}")
}

/// Load the metrics cache from disk.
///
/// Returns `None` on cold start (file absent) or on read/decompress error
/// (non-fatal). The caller should treat `None` as an empty cache and proceed.
pub fn read_metrics_cache(repo_root: &Path) -> Option<MetricsCache> {
    let path = cache_path(repo_root);
    if !path.exists() {
        return None;
    }
    match load_compressed_json(&path) {
        Ok(cache) => Some(cache),
        Err(e) => {
            eprintln!("warning: failed to load metrics cache (proceeding cold): {e}");
            None
        }
    }
}

fn load_compressed_json(path: &Path) -> Result<MetricsCache> {
    let compressed = std::fs::read(path)
        .with_context(|| format!("failed to read metrics cache: {}", path.display()))?;
    let bytes = zstd::decode_all(compressed.as_slice())
        .with_context(|| format!("failed to decompress metrics cache: {}", path.display()))?;
    let json = std::str::from_utf8(&bytes).context("metrics cache is not valid UTF-8")?;
    serde_json::from_str(json).context("failed to parse metrics cache JSON")
}

/// Write the metrics cache to disk (zstd level 3).
pub fn write_metrics_cache(repo_root: &Path, cache: &MetricsCache) -> Result<()> {
    let path = cache_path(repo_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }
    let json = serde_json::to_string(cache).context("failed to serialize metrics cache")?;
    let compressed =
        zstd::encode_all(json.as_bytes(), 3).context("failed to compress metrics cache")?;
    std::fs::write(&path, &compressed)
        .with_context(|| format!("failed to write metrics cache: {}", path.display()))
}

/// Evict cache entries whose content hash is not among `known_hashes`.
///
/// `known_hashes` are the content hashes seen during the current run; at most
/// `MAX_CACHED_HASHES` entries are retained overall. This bounds file size as
/// a repo's files (and thus their distinct content hashes) accumulate across
/// many runs, branches, or historic commits.
pub fn evict_old_entries(cache: &mut MetricsCache, known_hashes: &HashSet<String>) {
    // Key format: "v{version}:{hash}:{start}:{end}" — hash is the 2nd segment.
    cache.retain(|key, _| {
        key.split(':')
            .nth(1)
            .is_some_and(|hash| known_hashes.contains(hash))
    });
    if cache.len() > MAX_CACHED_HASHES {
        let drop_count = cache.len() - MAX_CACHED_HASHES;
        let drop_keys: Vec<String> = cache.keys().take(drop_count).cloned().collect();
        for key in drop_keys {
            cache.remove(&key);
        }
    }
}

/// Per-repo cache state held for the lifetime of the process, so a single
/// `analyze` run loads the on-disk cache once (not once per file) and writes
/// it back once at the end via [`flush_all`].
struct RepoState {
    cache: MetricsCache,
    known_hashes: HashSet<String>,
    dirty: bool,
}

type RepoStates = HashMap<PathBuf, Mutex<RepoState>>;

static REPO_STATES: OnceLock<Mutex<RepoStates>> = OnceLock::new();

fn repo_states() -> &'static Mutex<RepoStates> {
    REPO_STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Walk up from `start` looking for a `.git` directory; falls back to
/// `start`'s own directory (or `start` itself) if none is found. Scoped to
/// this module rather than depending on the CLI crate's own repo-root finder.
fn discover_repo_root(start: &Path) -> PathBuf {
    let start_dir = if start.is_dir() {
        Some(start)
    } else {
        start.parent()
    };
    let mut dir = start_dir;
    while let Some(d) = dir {
        if d.join(".git").exists() {
            return d.to_path_buf();
        }
        dir = d.parent();
    }
    start_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| start.to_path_buf())
}

/// Ensure a repo's slot exists in the process-wide cache state, loading it
/// from disk on first access, and return its repo root.
fn ensure_repo_slot(file_path: &Path) -> PathBuf {
    let repo_root = discover_repo_root(file_path);
    let mut map = repo_states().lock().unwrap_or_else(|e| e.into_inner());
    map.entry(repo_root.clone()).or_insert_with(|| {
        let cache = read_metrics_cache(&repo_root).unwrap_or_default();
        Mutex::new(RepoState {
            cache,
            known_hashes: HashSet::new(),
            dirty: false,
        })
    });
    repo_root
}

/// Look up a cached `RawMetrics` by its full cache key, recording
/// `content_hash` as "seen this run" regardless of hit/miss (so
/// [`flush_all`]'s eviction keeps entries for files analyzed this run even
/// if every one of their functions happened to be a cache hit).
///
/// `file_path` is used only to locate the repo root for the on-disk cache
/// file; the cache key itself is content-hash-based and independent of path.
pub fn peek(file_path: &Path, content_hash: &str, key: &str) -> Option<RawMetrics> {
    let repo_root = ensure_repo_slot(file_path);
    let map = repo_states().lock().unwrap_or_else(|e| e.into_inner());
    let repo_state_mutex = map
        .get(&repo_root)
        .expect("ensure_repo_slot just inserted this");
    let mut state = repo_state_mutex.lock().unwrap_or_else(|e| e.into_inner());
    state.known_hashes.insert(content_hash.to_string());
    state.cache.get(key).cloned()
}

/// Insert a freshly computed `RawMetrics` into the cache under `key`.
pub fn insert(file_path: &Path, content_hash: &str, key: &str, metrics: RawMetrics) {
    let repo_root = ensure_repo_slot(file_path);
    let map = repo_states().lock().unwrap_or_else(|e| e.into_inner());
    let repo_state_mutex = map
        .get(&repo_root)
        .expect("ensure_repo_slot just inserted this");
    let mut state = repo_state_mutex.lock().unwrap_or_else(|e| e.into_inner());
    state.known_hashes.insert(content_hash.to_string());
    state.cache.insert(key.to_string(), metrics);
    state.dirty = true;
}

/// Flush every repo's in-memory cache state (accumulated via [`peek`] and
/// [`insert`]) to disk, evicting entries not seen in this run.
///
/// Intended to be called once, at the end of a top-level `analyze` run.
pub fn flush_all() {
    let map = repo_states().lock().unwrap_or_else(|e| e.into_inner());
    for (repo_root, state_mutex) in map.iter() {
        let mut state = state_mutex.lock().unwrap_or_else(|e| e.into_inner());
        if !state.dirty {
            continue;
        }
        let known_hashes = state.known_hashes.clone();
        evict_old_entries(&mut state.cache, &known_hashes);
        if let Err(e) = write_metrics_cache(repo_root, &state.cache) {
            eprintln!("warning: failed to write metrics cache: {e}");
        }
        state.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_includes_version_hash_and_lines() {
        let key = cache_key("abc123", 10, 20);
        assert_eq!(key, format!("v{CACHE_VERSION}:abc123:10:20"));
    }

    #[test]
    fn hash_content_is_deterministic_and_content_sensitive() {
        let h1 = hash_content("fn foo() {}");
        let h2 = hash_content("fn foo() {}");
        let h3 = hash_content("fn foo() { bar(); }");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    fn sample_metrics() -> RawMetrics {
        RawMetrics {
            cc: 1,
            nd: 1,
            fo: 0,
            ns: 0,
            loc: 1,
            callee_names: vec![],
        }
    }

    #[test]
    fn evict_drops_unknown_hashes() {
        let mut cache = MetricsCache::new();
        cache.insert(cache_key("known", 1, 2), sample_metrics());
        cache.insert(cache_key("stale", 1, 2), sample_metrics());
        let known: HashSet<String> = ["known".to_string()].into_iter().collect();
        evict_old_entries(&mut cache, &known);
        assert_eq!(cache.len(), 1);
        assert!(cache.contains_key(&cache_key("known", 1, 2)));
    }

    #[test]
    fn read_write_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = MetricsCache::new();
        cache.insert(
            cache_key("abc", 1, 2),
            RawMetrics {
                cc: 3,
                nd: 2,
                fo: 1,
                ns: 0,
                loc: 10,
                callee_names: vec!["foo".to_string()],
            },
        );
        write_metrics_cache(dir.path(), &cache).unwrap();
        let loaded = read_metrics_cache(dir.path()).unwrap();
        assert_eq!(loaded.len(), 1);
        let entry = loaded.get(&cache_key("abc", 1, 2)).unwrap();
        assert_eq!(entry.cc, 3);
        assert_eq!(entry.callee_names, vec!["foo".to_string()]);
    }

    #[test]
    fn peek_then_insert_hits_cache_on_second_lookup_same_content() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("a.ts");
        std::fs::write(&file_path, "function f() {}").unwrap();
        let hash = hash_content("function f() {}");
        let key = cache_key(&hash, 1, 1);

        assert!(peek(&file_path, &hash, &key).is_none());
        insert(&file_path, &hash, &key, sample_metrics());
        let hit = peek(&file_path, &hash, &key);
        assert_eq!(hit.unwrap().cc, sample_metrics().cc);

        // Different content hash (simulated file change) must miss.
        let other_hash = hash_content("function f() { g(); }");
        let other_key = cache_key(&other_hash, 1, 1);
        assert!(peek(&file_path, &other_hash, &other_key).is_none());
    }
}
