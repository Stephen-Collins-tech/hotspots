// Build script to inject version information from git tags
//
// Standard Rust practice: Use a build script with git describe
// Alternative: Use 'vergen' or 'git-version' crates (adds dependency)
//
// This approach:
// - No runtime dependencies
// - Works in most environments (requires git at build time)
// - Falls back gracefully if git unavailable

use std::process::Command;

fn main() {
    // Get version from git describe, fallback to CARGO_PKG_VERSION
    let version = get_git_version().unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    println!("cargo:rustc-env=HOTSPOTS_VERSION={}", version);
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
    println!("cargo:rerun-if-changed=.git/refs/tags");
}

fn get_git_version() -> Option<String> {
    // Try to get version from git describe (prefers tags)
    // This will return something like "v0.1.0" or "v0.1.0-5-gabc123" or "abc123-dirty"
    let output = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()?;

    if output.status.success() {
        let version = String::from_utf8(output.stdout).ok()?;
        let version = version.trim();

        // If it's a clean tag (starts with v and no suffix), use it directly
        if version.starts_with('v') && !version.contains('-') {
            // Clean tag like "v0.1.0"
            Some(version.trim_start_matches('v').to_string())
        } else if version.starts_with('v') {
            // Tag with commits/dirty like "v0.1.0-5-gabc123" or "v0.1.0-5-gabc123-dirty"
            // Extract just the version part (everything after 'v' up to first '-')
            if let Some(dash_pos) = version.find('-') {
                Some(version[1..dash_pos].to_string())
            } else {
                Some(version.trim_start_matches('v').to_string())
            }
        } else {
            // Not a tagged version, use CARGO_PKG_VERSION with git info
            let base_version = env!("CARGO_PKG_VERSION");
            if let Some(clean_version) = version.strip_suffix("-dirty") {
                Some(format!("{}-{}-dirty", base_version, clean_version))
            } else {
                Some(format!("{}-{}", base_version, version))
            }
        }
    } else {
        None
    }
}
