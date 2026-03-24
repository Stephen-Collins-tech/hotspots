//! `hotspots init` — project setup helpers

pub(crate) fn handle_init(hooks: bool) -> anyhow::Result<()> {
    if hooks {
        print_hooks();
    } else {
        eprintln!("Nothing to do. Try: hotspots init --hooks");
    }
    Ok(())
}

fn print_hooks() {
    println!(
        r#"# ── SETUP (run once before enabling the hook) ───────────────────────
# Delta mode compares against the last persisted snapshot. On a fresh
# repo with no snapshots yet, policy evaluation is silently skipped.
# Seed the baseline first:
#
#   hotspots analyze . --mode snapshot
#
# ── pre-commit hook (pre-commit framework) ──────────────────────────
# Add to .pre-commit-config.yaml:

repos:
  - repo: local
    hooks:
      - id: hotspots
        name: hotspots risk check
        language: system
        entry: hotspots analyze . --mode delta --policy --format text
        pass_filenames: false
        stages: [pre-push]

# ── raw shell hook (no framework) ────────────────────────────────────
# Save as .git/hooks/pre-push and run: chmod +x .git/hooks/pre-push

#!/usr/bin/env sh
set -e
hotspots analyze . --mode delta --policy --format text
"#
    );
}
