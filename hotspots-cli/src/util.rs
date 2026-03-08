use anyhow::Context;
use std::path::{Path, PathBuf};

/// Map a driving dimension label to its (label, action) pair.
pub(crate) fn driving_dimension(
    func: &hotspots_core::snapshot::FunctionSnapshot,
) -> (&'static str, &'static str) {
    let label = hotspots_core::snapshot::normalize_driver_label(
        func.driver.as_deref().unwrap_or("composite"),
    );
    (label, hotspots_core::snapshot::driver_action(label))
}

/// Truncate a string to at most `max_len` characters, appending `...` if truncated.
pub(crate) fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Write an HTML report to `path` using an atomic temp-rename pattern.
pub(crate) fn write_html_report(path: &Path, html: &str) -> anyhow::Result<()> {
    use std::fs;

    if let Some(parent) = path.parent() {
        let parent_str = parent.display().to_string();
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent_str))?;
    }

    let temp_path = path.with_extension("html.tmp");
    std::fs::write(&temp_path, html)
        .with_context(|| format!("Failed to write temporary file: {}", temp_path.display()))?;
    std::fs::rename(&temp_path, path)
        .with_context(|| format!("Failed to rename temporary file to: {}", path.display()))?;

    Ok(())
}

/// Find the git repository root by walking up from `start_path`.
pub(crate) fn find_repo_root(start_path: &Path) -> anyhow::Result<PathBuf> {
    let mut current = if start_path.is_file() {
        start_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("invalid file path"))?
            .to_path_buf()
    } else {
        start_path.to_path_buf()
    };

    loop {
        if current.join(".git").exists() {
            return Ok(current);
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => anyhow::bail!("not in a git repository (no .git directory found)"),
        }
    }
}
