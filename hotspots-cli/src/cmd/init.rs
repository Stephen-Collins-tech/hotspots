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
