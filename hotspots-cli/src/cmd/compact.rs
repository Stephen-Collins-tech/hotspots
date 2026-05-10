use crate::util::find_repo_root;
use hotspots_core::compact;
use hotspots_core::snapshot;

pub(crate) fn handle_compact(level: u32, dry_run: bool) -> anyhow::Result<()> {
    if level > 2 {
        anyhow::bail!("compaction level must be 0, 1, or 2 (got {})", level);
    }

    let repo_root = find_repo_root(&std::env::current_dir()?)?;

    if level == 0 {
        let index_path = snapshot::index_path(&repo_root);
        let mut index = snapshot::Index::load_or_new(&index_path)?;
        let old_level = index.compaction_level();
        if !dry_run {
            index.set_compaction_level(0);
            snapshot::atomic_write(&index_path, &index.to_json()?)?;
        }
        if dry_run {
            println!(
                "Dry-run: would set compaction level to 0 (currently {})",
                old_level
            );
        } else {
            println!("Compaction level set to 0 (was {})", old_level);
        }
        return Ok(());
    }

    let result = if level == 1 {
        compact::compact_to_level1(&repo_root, dry_run, 1)?
    } else {
        compact::compact_to_level2(&repo_root, dry_run)?
    };

    let prefix = if dry_run { "Dry-run: would " } else { "" };

    if level == 1 {
        if result.converted_count == 0 {
            println!("Nothing to compact (all intermediate snapshots are already deltas).");
        } else {
            println!(
                "{prefix}convert {} snapshot(s) to delta encoding",
                result.converted_count
            );
            if result.bytes_freed > 0 {
                println!(
                    "Estimated storage reduction: {}",
                    format_bytes(result.bytes_freed)
                );
            }
        }
    } else {
        if result.dropped_count == 0 {
            println!("Nothing to compact (all snapshots have band changes).");
        } else {
            println!(
                "{prefix}drop {} snapshot(s) with no band changes",
                result.dropped_count
            );
            if result.bytes_freed > 0 {
                println!(
                    "Estimated storage reduction: {}",
                    format_bytes(result.bytes_freed)
                );
            }
        }
    }

    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1_024 {
        format!("{:.1} KB", bytes as f64 / 1_024.0)
    } else {
        format!("{} B", bytes)
    }
}
