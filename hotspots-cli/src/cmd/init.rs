//! `hotspots init` — project setup helpers

use std::path::Path;

pub(crate) fn handle_init(hooks: bool, ci: bool) -> anyhow::Result<()> {
    match (hooks, ci) {
        (true, _) => print_hooks(),
        (_, true) => write_ci_workflow()?,
        _ => eprintln!("Nothing to do. Try: hotspots init --hooks  or  hotspots init --ci"),
    }
    Ok(())
}

fn print_hooks() {
    // Print the pre-commit YAML snippet
    println!(
        r#"# ── SETUP (run once before enabling the hook) ───────────────────────
# Delta mode compares against the last persisted snapshot. On a fresh
# repo with no snapshots yet, policy evaluation is silently skipped.
# Seed the baseline first:
#
#   hotspots analyze . --mode snapshot

# ── Option 1: pre-commit framework ───────────────────────────────────
# Add the following to .pre-commit-config.yaml:

repos:
  - repo: local
    hooks:
      - id: hotspots
        name: hotspots risk check
        language: system
        entry: hotspots analyze . --mode delta --policy --format text
        pass_filenames: false
        stages: [pre-push]"#
    );

    // Print the raw shell hook as a standalone block so users can copy it
    // directly to .git/hooks/pre-push without including the YAML above.
    println!(
        r#"
# ── Option 2: raw shell hook ─────────────────────────────────────────
# Save the lines below (starting with the shebang) as .git/hooks/pre-push
# and run: chmod +x .git/hooks/pre-push

#!/usr/bin/env sh
set -e
hotspots analyze . --mode delta --policy --format text"#
    );
}

/// The GitHub Actions workflow content.
///
/// Strategy:
///   - Push to main/master → snapshot mode: analyzes the full tree and persists
///     the snapshot. The cache is saved under `hotspots-{sha}`.
///   - Pull request → delta mode: restores the cache for the PR base branch
///     commit (the snapshot built when that commit landed on main), then runs
///     delta + policy against it.
///
/// On the very first run there is no warm cache yet. The delta run will report
/// "no parent snapshot" and skip policy gating; policy kicks in from the second
/// PR onward once the main-branch snapshot is cached.
const GITHUB_WORKFLOW: &str = r#"name: Hotspots

on:
  pull_request:
  push:
    branches:
      - main
      - master

permissions:
  contents: read

jobs:
  hotspots:
    name: Analyze code hotspots
    runs-on: ubuntu-latest

    steps:
      - name: Checkout repository
        uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Install Hotspots CLI
        run: |
          curl -fsSL https://raw.githubusercontent.com/Stephen-Collins-tech/hotspots/main/install.sh | sh
          echo "$HOME/.local/bin" >> "$GITHUB_PATH"

      # On push-to-main we save the snapshot under hotspots-{sha}.
      # On a PR we restore the snapshot for the base-branch commit so that
      # delta mode has a baseline to diff against.
      - name: Restore snapshot cache
        uses: actions/cache@v4
        with:
          path: .hotspots/
          key: hotspots-${{ github.sha }}
          restore-keys: |
            hotspots-${{ github.event.pull_request.base.sha }}
            hotspots-

      - name: Analyze (snapshot on main, delta on PR)
        run: |
          if [ "${{ github.event_name }}" = "pull_request" ]; then
            hotspots analyze . --mode delta --policy
          else
            # --mode snapshot requires --format json (or --explain) when not
            # printing text; the snapshot is persisted to .hotspots/ either way.
            hotspots analyze . --mode snapshot --format json > /dev/null
          fi
"#;

fn write_ci_workflow() -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let repo_root = find_repo_root(&cwd).unwrap_or(cwd);
    write_ci_workflow_at(&repo_root)
}

fn write_ci_workflow_at(repo_root: &Path) -> anyhow::Result<()> {
    let workflows_dir = repo_root.join(".github").join("workflows");
    let workflow_path = workflows_dir.join("hotspots.yml");

    if workflow_path.exists() {
        anyhow::bail!(
            "{} already exists. Remove it first or edit it manually.",
            workflow_path.display()
        );
    }

    std::fs::create_dir_all(&workflows_dir)?;
    std::fs::write(&workflow_path, GITHUB_WORKFLOW)?;

    eprintln!("Wrote {}", workflow_path.display());
    eprintln!();
    eprintln!("How it works:");
    eprintln!("  push to main  →  hotspots analyze --mode snapshot  (builds the baseline)");
    eprintln!("  pull request  →  hotspots analyze --mode delta --policy  (gates on risk)");
    eprintln!();
    eprintln!("Commit the file and push to enable the check on your next PR.");

    Ok(())
}

/// Walk up from `start` until a `.git` directory is found.
fn find_repo_root(start: &Path) -> Option<std::path::PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn find_repo_root_finds_git_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join(".git")).unwrap();
        let nested = root.join("a").join("b");
        fs::create_dir_all(&nested).unwrap();

        let found = find_repo_root(&nested);
        assert_eq!(found.as_deref(), Some(root));
    }

    #[test]
    fn find_repo_root_returns_none_without_git() {
        let tmp = tempfile::tempdir().unwrap();
        // no .git directory — must return None
        assert!(find_repo_root(tmp.path()).is_none());
    }

    #[test]
    fn write_ci_workflow_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        write_ci_workflow_at(tmp.path()).unwrap();

        let workflow_path = tmp
            .path()
            .join(".github")
            .join("workflows")
            .join("hotspots.yml");
        assert!(workflow_path.exists(), "workflow file should be created");

        let content = fs::read_to_string(&workflow_path).unwrap();
        assert!(content.contains("name: Hotspots"));
        assert!(content.contains("--mode snapshot"));
        assert!(content.contains("--mode delta --policy"));
    }

    #[test]
    fn write_ci_workflow_fails_if_already_exists() {
        let tmp = tempfile::tempdir().unwrap();
        write_ci_workflow_at(tmp.path()).unwrap();

        let err = write_ci_workflow_at(tmp.path()).unwrap_err();
        assert!(
            err.to_string().contains("already exists"),
            "expected 'already exists' error, got: {err}"
        );
    }
}
