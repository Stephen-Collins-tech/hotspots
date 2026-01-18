//! Faultline CLI - command-line interface for TypeScript analysis

#![deny(warnings)]

// Global invariants enforced:
// - Deterministic output ordering
// - Identical input yields byte-for-byte identical output

use anyhow::Context;
use clap::{Parser, Subcommand};
use faultline_core::{analyze, render_json, render_text, AnalysisOptions};
use faultline_core::{delta, git, prune};
use faultline_core::snapshot::{self, Snapshot};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "faultline")]
#[command(about = "Static analysis tool for TypeScript functions")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze TypeScript files
    Analyze {
        /// Path to TypeScript file or directory
        path: PathBuf,
        
        /// Output format
        #[arg(long, default_value = "text")]
        format: OutputFormat,
        
        /// Output mode (snapshot or delta)
        /// When not specified, preserves existing text/JSON output behavior
        #[arg(long)]
        mode: Option<OutputMode>,
        
        /// Show only top N results
        #[arg(long)]
        top: Option<usize>,
        
        /// Minimum LRS threshold
        #[arg(long)]
        min_lrs: Option<f64>,
    },
    /// Prune unreachable snapshots
    Prune {
        /// Prune unreachable snapshots (must be explicitly specified)
        #[arg(long)]
        unreachable: bool,
        
        /// Only prune commits older than this many days
        #[arg(long)]
        older_than: Option<u64>,
        
        /// Dry-run mode (report what would be pruned without actually deleting)
        #[arg(long)]
        dry_run: bool,
    },
    /// Compact history to reduce storage
    Compact {
        /// Compaction level (0 = full snapshots, 1 = deltas only, 2 = band transitions only)
        #[arg(long)]
        level: u32,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum OutputMode {
    Snapshot,
    Delta,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Analyze { path, format, mode, top, min_lrs } => {
            // Normalize path to absolute
            let normalized_path = if path.is_relative() {
                std::env::current_dir()?.join(&path)
            } else {
                path
            };
            
            // Validate path exists
            if !normalized_path.exists() {
                anyhow::bail!("Path does not exist: {}", normalized_path.display());
            }
            
            // If mode is specified, use snapshot/delta mode
            if let Some(output_mode) = mode {
                return handle_mode_output(&normalized_path, output_mode, format, top, min_lrs);
            }
            
            // Default behavior: preserve existing text/JSON output
            let options = AnalysisOptions {
                min_lrs,
                top_n: top,
            };
            
            // Analyze
            let reports = analyze(&normalized_path, options)?;
            
            // Render output
            match format {
                OutputFormat::Text => {
                    print!("{}", render_text(&reports));
                }
                OutputFormat::Json => {
                    println!("{}", render_json(&reports));
                }
            }
        }
        Commands::Prune { unreachable, older_than, dry_run } => {
            if !unreachable {
                anyhow::bail!("--unreachable flag must be specified to prune snapshots");
            }

            // Find repository root (search up from current directory)
            let repo_root = find_repo_root(&std::env::current_dir()?)?;

            // Build prune options
            let options = prune::PruneOptions {
                ref_patterns: vec!["refs/heads/*".to_string()], // Default: local branches only
                older_than_days: older_than,
                dry_run,
            };

            // Execute pruning
            let result = prune::prune_unreachable(&repo_root, options)?;

            // Print results
            if dry_run {
                println!("Dry-run: Would prune {} snapshots", result.pruned_count);
            } else {
                println!("Pruned {} snapshots", result.pruned_count);
            }
            
            if !result.pruned_shas.is_empty() {
                println!("\nPruned commit SHAs:");
                for sha in &result.pruned_shas {
                    println!("  {}", sha);
                }
            }

            println!("\nReachable snapshots: {}", result.reachable_count);
            if result.unreachable_kept_count > 0 {
                println!("Unreachable snapshots kept (due to age filter): {}", result.unreachable_kept_count);
            }
        }
        Commands::Compact { level } => {
            // Validate compaction level
            if level > 2 {
                anyhow::bail!("compaction level must be 0, 1, or 2 (got {})", level);
            }

            // Find repository root (search up from current directory)
            let repo_root = find_repo_root(&std::env::current_dir()?)?;

            // Load index
            let index_path = snapshot::index_path(&repo_root);
            let mut index = snapshot::Index::load_or_new(&index_path)?;

            // Set compaction level
            let old_level = index.compaction_level();
            index.set_compaction_level(level);

            // Write updated index atomically
            let index_json = index.to_json()?;
            snapshot::atomic_write(&index_path, &index_json)?;

            println!("Compaction level set to {} (was {})", level, old_level);
            
            // Note: Actual compaction to levels 1 or 2 is not yet implemented
            // This only sets the metadata. Level 0 (full snapshots) is the current implementation.
            if level > 0 {
                println!("Note: Compaction to level {} is not yet implemented. Only metadata was updated.", level);
            }
        }
    }
    
    Ok(())
}

/// Handle snapshot or delta mode output
fn handle_mode_output(
    path: &PathBuf,
    mode: OutputMode,
    format: OutputFormat,
    top: Option<usize>,
    min_lrs: Option<f64>,
) -> anyhow::Result<()> {
    // Find repository root (search up from current path)
    let repo_root = find_repo_root(path)?;
    
    // Analyze codebase
    let options = AnalysisOptions {
        min_lrs,
        top_n: top,
    };
    let reports = analyze(path, options)?;
    
    // Detect PR context (best-effort, CI env vars only)
    let pr_context = git::detect_pr_context();
    let is_mainline = !pr_context.is_pr;
    
    match mode {
        OutputMode::Snapshot => {
            // Extract git context
            let git_context = git::extract_git_context()
                .context("failed to extract git context (required for snapshot mode)")?;
            
            // Create snapshot
            let snapshot = Snapshot::new(git_context, reports);
            
            // Persist snapshot only in mainline mode (not in PR mode)
            if is_mainline {
                snapshot::persist_snapshot(&repo_root, &snapshot)
                    .context("failed to persist snapshot")?;
                snapshot::append_to_index(&repo_root, &snapshot)
                    .context("failed to update index")?;
            }
            
            // Emit snapshot JSON
            match format {
                OutputFormat::Json => {
                    let json = snapshot.to_json()?;
                    println!("{}", json);
                }
                OutputFormat::Text => {
                    // Text format not supported for snapshot initially
                    anyhow::bail!("text format is not supported for snapshot mode (use --format json)");
                }
            }
        }
        OutputMode::Delta => {
            // Extract git context
            let git_context = git::extract_git_context()
                .context("failed to extract git context (required for delta mode)")?;
            
            // Create snapshot
            let snapshot = Snapshot::new(git_context, reports);
            
            // Compute delta
            let delta = if pr_context.is_pr {
                // PR mode: compare vs merge-base
                compute_pr_delta(&repo_root, &snapshot)?
            } else {
                // Mainline mode: compare vs direct parent (parents[0])
                delta::compute_delta(&repo_root, &snapshot)?
            };
            
            // Emit delta JSON
            match format {
                OutputFormat::Json => {
                    let json = delta.to_json()?;
                    println!("{}", json);
                }
                OutputFormat::Text => {
                    // Text format not supported for delta initially
                    anyhow::bail!("text format is not supported for delta mode (use --format json)");
                }
            }
        }
    }
    
    Ok(())
}

/// Compute delta for PR mode (compares vs merge-base)
///
/// Resolves merge-base and compares current snapshot against it.
/// Falls back to direct parent if merge-base cannot be resolved (with warning).
fn compute_pr_delta(repo_root: &std::path::Path, snapshot: &Snapshot) -> anyhow::Result<delta::Delta> {
    // Try to resolve merge-base
    let merge_base_sha = git::resolve_merge_base_auto();
    
    let parent = if let Some(sha) = &merge_base_sha {
        // Load merge-base snapshot
        match delta::load_parent_snapshot(repo_root, sha)? {
            Some(parent_snapshot) => Some(parent_snapshot),
            None => {
                // Merge-base snapshot not found - fall back to direct parent with warning
                eprintln!("Warning: merge-base snapshot not found, falling back to direct parent");
                let parent_sha = snapshot.commit.parents.first();
                if let Some(sha) = parent_sha {
                    delta::load_parent_snapshot(repo_root, sha)?
                } else {
                    None
                }
            }
        }
    } else {
        // Merge-base resolution failed - fall back to direct parent with warning
        eprintln!("Warning: failed to resolve merge-base, falling back to direct parent");
        let parent_sha = snapshot.commit.parents.first();
        if let Some(sha) = parent_sha {
            delta::load_parent_snapshot(repo_root, sha)?
        } else {
            None
        }
    };
    
    delta::Delta::new(snapshot, parent.as_ref())
}

/// Find git repository root by searching up the directory tree
fn find_repo_root(start_path: &PathBuf) -> anyhow::Result<PathBuf> {
    let mut current = if start_path.is_file() {
        start_path.parent()
            .ok_or_else(|| anyhow::anyhow!("invalid file path"))?
            .to_path_buf()
    } else {
        start_path.clone()
    };
    
    loop {
        let git_dir = current.join(".git");
        if git_dir.exists() {
            return Ok(current);
        }
        
        // Move up one directory
        match current.parent() {
            Some(parent) => {
                current = parent.to_path_buf();
            }
            None => {
                anyhow::bail!("not in a git repository (no .git directory found)");
            }
        }
    }
}
