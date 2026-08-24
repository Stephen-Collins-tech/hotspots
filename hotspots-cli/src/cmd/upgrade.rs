//! `hotspots upgrade` — check whether a newer release is available

use serde::Deserialize;

const RELEASES_URL: &str =
    "https://api.github.com/repos/Stephen-Collins-tech/hotspots/releases/latest";

#[derive(Deserialize)]
struct LatestRelease {
    tag_name: String,
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

fn current_version() -> String {
    env!("HOTSPOTS_VERSION").to_string()
}

fn fetch_latest_version() -> anyhow::Result<String> {
    let body: LatestRelease = ureq::get(RELEASES_URL)
        .set("User-Agent", "hotspots-cli")
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
fn upgrade_command() -> String {
    let exe_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    match exe_path {
        p if p.contains("/.cargo/") => "cargo install hotspots-cli --force".to_string(),
        p if p.contains("node_modules") || p.contains(".nvm") => {
            "npm install -g @stephencollinstech/hotspots@latest".to_string()
        }
        p if p.contains("Cellar") || p.contains("homebrew") => "brew upgrade hotspots".to_string(),
        _ => "curl -fsSL https://raw.githubusercontent.com/Stephen-Collins-tech/hotspots/main/install.sh | sh".to_string(),
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
}
