use crate::util::find_repo_root;
use hotspots_core::{compact, snapshot};

pub(crate) fn handle_compact(level: u32, dry_run: bool, keep_full: usize) -> anyhow::Result<()> {
    if level > 2 {
        anyhow::bail!("compaction level must be 0, 1, or 2 (got {})", level);
    }

    let repo_root = find_repo_root(&std::env::current_dir()?)?;

    match level {
        0 => {
            if dry_run {
                println!("Dry-run: would reset compaction level to 0 (no-op for files)");
                return Ok(());
            }
            let index_path = snapshot::index_path(&repo_root);
            let mut index = snapshot::Index::load_or_new(&index_path)?;
            let old = index.compaction_level();
            index.set_compaction_level(0);
            snapshot::atomic_write(&index_path, &index.to_json()?)?;
            println!("Compaction level set to 0 (was {})", old);
        }
        1 => {
            if dry_run {
                println!("Dry-run: compact level 1 (keep_full={})", keep_full);
            }
            let stats = compact::compact_level1(&repo_root, keep_full, dry_run)?;
            if dry_run {
                println!(
                    "Would compact {} snapshot(s) to deltas, saving ~{} bytes (~{} KB)",
                    stats.snapshots_compacted,
                    stats.bytes_saved(),
                    stats.bytes_saved() / 1024,
                );
            } else {
                println!(
                    "Compacted {} snapshot(s) to deltas, saved {} bytes ({} KB)",
                    stats.snapshots_compacted,
                    stats.bytes_saved(),
                    stats.bytes_saved() / 1024,
                );
            }
        }
        2 => {
            // Level 2: first discard no-band-change snapshots, then delta-compress the rest.
            if dry_run {
                println!(
                    "Dry-run: compact level 2 (keep_full={}, then remove no-band-change snapshots)",
                    keep_full
                );
            }
            let stats2 = compact::compact_level2(&repo_root, dry_run)?;
            let stats1 = compact::compact_level1(&repo_root, keep_full, dry_run)?;

            let total_saved = stats1.bytes_saved() + stats2.bytes_saved();
            if dry_run {
                println!(
                    "Would remove {} snapshot(s) (no band change), compact {} snapshot(s) to deltas",
                    stats2.snapshots_removed, stats1.snapshots_compacted,
                );
                println!(
                    "Total projected savings: ~{} bytes (~{} KB)",
                    total_saved,
                    total_saved / 1024,
                );
            } else {
                println!(
                    "Removed {} snapshot(s) (no band change), compacted {} snapshot(s) to deltas",
                    stats2.snapshots_removed, stats1.snapshots_compacted,
                );
                println!(
                    "Total saved: {} bytes ({} KB)",
                    total_saved,
                    total_saved / 1024,
                );
            }
        }
        _ => unreachable!(),
    }

    Ok(())
}
