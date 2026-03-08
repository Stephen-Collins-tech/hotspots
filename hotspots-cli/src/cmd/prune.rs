use crate::util::find_repo_root;
use hotspots_core::prune;

pub(crate) fn handle_prune(
    unreachable: bool,
    older_than: Option<u64>,
    dry_run: bool,
) -> anyhow::Result<()> {
    if !unreachable {
        anyhow::bail!("--unreachable flag must be specified to prune snapshots");
    }

    let repo_root = find_repo_root(&std::env::current_dir()?)?;
    let options = prune::PruneOptions {
        ref_patterns: vec!["refs/heads/*".to_string()],
        older_than_days: older_than,
        dry_run,
    };
    let result = prune::prune_unreachable(&repo_root, options)?;

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
        println!(
            "Unreachable snapshots kept (due to age filter): {}",
            result.unreachable_kept_count
        );
    }

    Ok(())
}
