use crate::util::find_repo_root;
use hotspots_core::snapshot;

pub(crate) fn handle_compact(level: u32) -> anyhow::Result<()> {
    if level > 2 {
        anyhow::bail!("compaction level must be 0, 1, or 2 (got {})", level);
    }
    if level > 0 {
        anyhow::bail!(
            "compaction to level {} is not yet implemented (only level 0 is supported)",
            level
        );
    }

    let repo_root = find_repo_root(&std::env::current_dir()?)?;
    let index_path = snapshot::index_path(&repo_root);
    let mut index = snapshot::Index::load_or_new(&index_path)?;
    let old_level = index.compaction_level();
    index.set_compaction_level(level);
    let index_json = index.to_json()?;
    snapshot::atomic_write(&index_path, &index_json)?;

    println!("Compaction level set to {} (was {})", level, old_level);
    Ok(())
}
