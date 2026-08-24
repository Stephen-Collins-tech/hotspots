//! `hotspots upgrade` — check whether a newer release is available, and the
//! passive per-run notice (see `maybe_print_update_notice`) that surfaces the
//! same check before any other command's output.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const RELEASES_URL: &str =
    "https://api.github.com/repos/Stephen-Collins-tech/hotspots/releases/latest";
const USER_AGENT: &str = "hotspots-cli";
const REQUEST_TIMEOUT: Duration = Duration::from_millis(1500);

const CARGO_UPGRADE_CMD: &str = "cargo install hotspots-cli --force";
const NPM_UPGRADE_CMD: &str = "npm install -g @stephencollinstech/hotspots@latest";
const BREW_UPGRADE_CMD: &str = "brew upgrade hotspots";
const CURL_UPGRADE_CMD: &str =
    "curl -fsSL https://raw.githubusercontent.com/Stephen-Collins-tech/hotspots/main/install.sh | sh";

/// Passive per-run notice is re-checked against GitHub at most this often;
/// otherwise the cached result from `~/.hotspots/update_check.json` is used.
const NOTICE_CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const NOTICE_CACHE_FILE: &str = "update_check.json";

#[derive(Deserialize)]
struct LatestRelease {
    tag_name: String,
}

#[derive(Serialize, Deserialize, Default)]
struct NoticeCache {
    last_checked_unix: u64,
    latest_version: Option<String>,
}

pub(crate) fn handle_upgrade() -> anyhow::Result<()> {
    let current = current_version();
    let latest = fetch_latest_version()?;

    match compare_versions(&current, &latest) {
        std::cmp::Ordering::Less => {
            println!("A newer version of hotspots is available: {latest} (current: {current})");
            println!();
            println!("Upgrade with:");
            println!("  {}", upgrade_command());
        }
        std::cmp::Ordering::Equal => {
            println!("hotspots {current} is up to date.");
        }
        std::cmp::Ordering::Greater => {
            println!("hotspots {current} is newer than the latest published release ({latest}).");
        }
    }

    Ok(())
}

/// Best-effort passive check, printed to stderr before any other command
/// output. Never fails or blocks noticeably: network errors are swallowed,
/// and the real GitHub check only runs at most once per `NOTICE_CHECK_INTERVAL`
/// (cached in `~/.hotspots/update_check.json`), so most invocations do no
/// network I/O at all.
pub(crate) fn maybe_print_update_notice() {
    let Some(cache_path) = notice_cache_path() else {
        return;
    };
    let now = now_unix();
    let mut cache = read_notice_cache(&cache_path).unwrap_or_default();

    if now.saturating_sub(cache.last_checked_unix) >= NOTICE_CHECK_INTERVAL.as_secs() {
        if let Ok(latest) = fetch_latest_version() {
            cache.latest_version = Some(latest);
        }
        cache.last_checked_unix = now;
        let _ = write_notice_cache(&cache_path, &cache);
    }

    if let Some(latest) = &cache.latest_version {
        let current = current_version();
        if compare_versions(&current, latest) == std::cmp::Ordering::Less {
            eprintln!("hotspots {current} \u{2192} {latest} available. Run `hotspots upgrade` for details.");
            eprintln!();
        }
    }
}

fn notice_cache_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(
        PathBuf::from(home)
            .join(".hotspots")
            .join(NOTICE_CACHE_FILE),
    )
}

fn read_notice_cache(path: &PathBuf) -> Option<NoticeCache> {
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn write_notice_cache(path: &PathBuf, cache: &NoticeCache) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string(cache)?)?;
    Ok(())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn current_version() -> String {
    env!("HOTSPOTS_VERSION").to_string()
}

fn fetch_latest_version() -> anyhow::Result<String> {
    let agent = ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build();

    let body: LatestRelease = agent
        .get(RELEASES_URL)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| anyhow::anyhow!("failed to reach GitHub releases: {e}"))?
        .into_json()
        .map_err(|e| anyhow::anyhow!("failed to parse GitHub releases response: {e}"))?;

    Ok(body.tag_name.trim_start_matches('v').to_string())
}

/// Compares dotted-numeric version strings (e.g. "1.34.1"). Any non-numeric
/// suffix (git describe output like "1.34.1-3-gabc123-dirty") is ignored for
/// comparison purposes by only looking at the leading numeric segments.
fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |v: &str| -> Vec<u64> {
        v.split('.')
            .map(|part| {
                part.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse::<u64>()
                    .unwrap_or(0)
            })
            .collect()
    };

    parse(a).cmp(&parse(b))
}

/// Best-effort detection of how this binary was installed, based on the
/// running executable's path, so the suggested command actually works.
fn upgrade_command() -> &'static str {
    let exe_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    match exe_path {
        p if p.contains("/.cargo/") => CARGO_UPGRADE_CMD,
        p if p.contains("node_modules") || p.contains(".nvm") => NPM_UPGRADE_CMD,
        p if p.contains("Cellar") || p.contains("homebrew") => BREW_UPGRADE_CMD,
        _ => CURL_UPGRADE_CMD,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_versions_orders_numerically() {
        assert_eq!(
            compare_versions("1.9.0", "1.10.0"),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_versions("1.34.1", "1.34.1"),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            compare_versions("2.0.0", "1.34.1"),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn compare_versions_ignores_git_describe_suffix() {
        assert_eq!(
            compare_versions("1.34.1-3-gabc123-dirty", "1.34.1"),
            std::cmp::Ordering::Equal
        );
    }

    #[test]
    fn notice_cache_round_trips_through_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(NOTICE_CACHE_FILE);

        assert!(read_notice_cache(&path).is_none());

        let cache = NoticeCache {
            last_checked_unix: 12345,
            latest_version: Some("9.9.9".to_string()),
        };
        write_notice_cache(&path, &cache).unwrap();

        let read_back = read_notice_cache(&path).unwrap();
        assert_eq!(read_back.last_checked_unix, 12345);
        assert_eq!(read_back.latest_version.as_deref(), Some("9.9.9"));
    }
}
