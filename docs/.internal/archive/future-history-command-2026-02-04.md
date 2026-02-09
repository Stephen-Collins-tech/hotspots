# Future Enhancement: Batch History Processing

**Status:** Out of scope for initial synthetic harness. Manual iteration is preferred for Step 1.

## Current State

Hotspots currently has **no command to process all commits in a repository's history** when analyzing a repo for the first time. This is **intentional** for the current phase.

### Why Manual Iteration Is Preferred (Step 1)

For the initial synthetic harness validation:

1. **Small, controlled repos** - 8-10 commits, linear history, one hotspot
2. **Explicit inspection** - Manual checkout forces inspection of each snapshot/delta
3. **Early validation** - Catch surprises early, validate assumptions about metrics
4. **No premature optimization** - Don't optimize workflow before validating signal

Manual process:
```bash
git checkout <sha>
hotspots analyze . --mode snapshot
hotspots analyze . --mode delta
# Inspect outputs, then move to next commit
```

This is a **feature, not a bug** for initial validation.

### Current Workflow (Manual)

To analyze an existing repo with git history, you must manually:

```bash
# Get all commit SHAs
git log --reverse --format="%H" > commits.txt

# For each commit:
git checkout <sha>
hotspots analyze . --mode snapshot --format json
git checkout main  # Return to main branch
```

This is tedious and error-prone for repos with many commits.

---

## Proposed Solution: `history` Command

### Command Design

```bash
# Process all commits in history
hotspots history --all

# Process commits since a date
hotspots history --since "2024-01-01"

# Process commits in a range
hotspots history --range <sha1>..<sha2>

# Process commits from a specific ref
hotspots history --from <ref>

# Skip commits that already have snapshots (idempotent)
hotspots history --all --skip-existing

# Dry-run: show what would be processed
hotspots history --all --dry-run
```

### Implementation Approach

**Option 1: Simple Sequential Processing**

```rust
// In hotspots-cli/src/main.rs
Commands::History {
    /// Process all commits
    #[arg(long)]
    all: bool,
    
    /// Process commits since date (YYYY-MM-DD)
    #[arg(long)]
    since: Option<String>,
    
    /// Process commits in range (sha1..sha2)
    #[arg(long)]
    range: Option<String>,
    
    /// Process commits from ref
    #[arg(long)]
    from: Option<String>,
    
    /// Skip commits that already have snapshots
    #[arg(long)]
    skip_existing: bool,
    
    /// Dry-run mode
    #[arg(long)]
    dry_run: bool,
}
```

**Implementation steps:**

1. Get commit list using `git rev-list`:
   ```rust
   // Get all commits in reverse chronological order (oldest first)
   let commits = git_at(repo_path, &["rev-list", "--reverse", "--all"])?;
   ```

2. For each commit:
   - Checkout commit (detached HEAD)
   - Check if snapshot exists (if `--skip-existing`)
   - Run analysis and create snapshot
   - Restore original branch/HEAD

3. Handle errors gracefully:
   - If analysis fails on a commit, log and continue
   - Restore original HEAD even on error

**Option 2: Use Existing Infrastructure**

Leverage `prune.rs`'s `git_at()` and `compute_reachable_commits()` functions:

```rust
// In hotspots-core/src/history.rs (new module)
pub fn process_history(
    repo_path: &Path,
    options: HistoryOptions
) -> Result<HistoryResult> {
    // Get commits to process
    let commits = match options.mode {
        HistoryMode::All => {
            git_at(repo_path, &["rev-list", "--reverse", "--all"])?
                .lines()
                .map(|s| s.trim().to_string())
                .collect()
        }
        HistoryMode::Since(date) => {
            // Use --since flag
            git_at(repo_path, &["rev-list", "--reverse", "--since", &date, "--all"])?
                .lines()
                .map(|s| s.trim().to_string())
                .collect()
        }
        HistoryMode::Range(range) => {
            git_at(repo_path, &["rev-list", "--reverse", &range])?
                .lines()
                .map(|s| s.trim().to_string())
                .collect()
        }
        HistoryMode::From(ref_name) => {
            git_at(repo_path, &["rev-list", "--reverse", ref_name])?
                .lines()
                .map(|s| s.trim().to_string())
                .collect()
        }
    };
    
    // Save current HEAD
    let original_head = git_at(repo_path, &["rev-parse", "HEAD"])?;
    let original_branch = git_at(repo_path, &["symbolic-ref", "--short", "HEAD"]).ok();
    
    let mut processed = 0;
    let mut skipped = 0;
    let mut errors = Vec::new();
    
    for commit_sha in commits {
        // Check if snapshot exists
        if options.skip_existing {
            let snapshot_path = snapshot::snapshot_path(repo_path, &commit_sha);
            if snapshot_path.exists() {
                skipped += 1;
                continue;
            }
        }
        
        if options.dry_run {
            println!("Would process: {}", commit_sha);
            continue;
        }
        
        // Checkout commit
        git_at(repo_path, &["checkout", &commit_sha])?;
        
        // Analyze and create snapshot
        match process_commit(repo_path, &commit_sha) {
            Ok(_) => processed += 1,
            Err(e) => {
                errors.push((commit_sha.clone(), e.to_string()));
                eprintln!("Error processing {}: {}", commit_sha, e);
            }
        }
    }
    
    // Restore original HEAD
    if let Some(branch) = original_branch {
        git_at(repo_path, &["checkout", &branch])?;
    } else {
        git_at(repo_path, &["checkout", &original_head])?;
    }
    
    Ok(HistoryResult {
        processed,
        skipped,
        errors,
    })
}

fn process_commit(repo_path: &Path, commit_sha: &str) -> Result<()> {
    // Extract git context
    let git_context = git::extract_git_context_at(repo_path)?;
    
    // Analyze codebase
    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    let reports = analyze(repo_path, options)?;
    
    // Create snapshot
    let snapshot = Snapshot::new(git_context, reports);
    
    // Persist snapshot
    snapshot::persist_snapshot(repo_path, &snapshot)?;
    snapshot::append_to_index(repo_path, &snapshot)?;
    
    Ok(())
}
```

---

## Safety Considerations

### 1. Preserve Working Directory

- Save current HEAD/branch before processing
- Restore after processing (even on error)
- Use `git checkout` in detached HEAD mode (safe, doesn't modify working tree if clean)

### 2. Handle Dirty Working Directory

```rust
// Check if working directory is clean
let status = git_at(repo_path, &["status", "--porcelain"])?;
if !status.is_empty() {
    anyhow::bail!("working directory is not clean. commit or stash changes first");
}
```

### 3. Idempotency

- `--skip-existing` flag checks if snapshot already exists
- Re-running is safe (snapshots are immutable)
- Can resume after interruption

### 4. Error Handling

- Continue processing even if one commit fails
- Log errors for review
- Return summary of processed/skipped/errors

---

## Example Usage

### Initial Setup of Existing Repo

```bash
# Process all commits in history
hotspots history --all

# Output:
# Processing 150 commits...
# ✓ Processed: 150
# ⚠ Skipped: 0 (already had snapshots)
# ✗ Errors: 0
```

### Incremental Updates

```bash
# Process only new commits (skip existing)
hotspots history --all --skip-existing

# Process commits since last month
hotspots history --since "2024-01-01" --skip-existing
```

### Specific Range

```bash
# Process commits in a feature branch
hotspots history --from feature-branch

# Process commits between two SHAs
hotspots history --range abc123..def456
```

### Dry-Run

```bash
# See what would be processed
hotspots history --all --dry-run

# Output:
# Would process: abc123 (2024-01-01)
# Would process: def456 (2024-01-02)
# ...
```

---

## Alternative: Shell Script Wrapper

Until the command is implemented, a simple shell script can fill the gap:

```bash
#!/bin/bash
# hotspots-history.sh

set -e

REPO_DIR="${1:-.}"
SKIP_EXISTING="${2:-false}"

cd "$REPO_DIR"

# Save current branch
ORIGINAL_BRANCH=$(git symbolic-ref --short HEAD 2>/dev/null || echo "HEAD")
ORIGINAL_HEAD=$(git rev-parse HEAD)

# Get all commits
COMMITS=$(git rev-list --reverse --all)

PROCESSED=0
SKIPPED=0

for SHA in $COMMITS; do
    # Check if snapshot exists
    if [ "$SKIP_EXISTING" = "true" ]; then
        if [ -f ".hotspots/snapshots/${SHA}.json" ]; then
            echo "Skipping ${SHA} (snapshot exists)"
            SKIPPED=$((SKIPPED + 1))
            continue
        fi
    fi
    
    echo "Processing ${SHA}..."
    
    # Checkout commit
    git checkout "$SHA" > /dev/null 2>&1
    
    # Create snapshot
    if hotspots analyze . --mode snapshot --format json > /dev/null 2>&1; then
        PROCESSED=$((PROCESSED + 1))
    else
        echo "Error processing ${SHA}"
    fi
done

# Restore original branch
if [ "$ORIGINAL_BRANCH" != "HEAD" ]; then
    git checkout "$ORIGINAL_BRANCH" > /dev/null 2>&1
else
    git checkout "$ORIGINAL_HEAD" > /dev/null 2>&1
fi

echo "Processed: $PROCESSED"
echo "Skipped: $SKIPPED"
```

**Usage:**
```bash
chmod +x hotspots-history.sh
./hotspots-history.sh . true  # Skip existing snapshots
```

---

## When to Implement

**Do not implement yet.** This feature should only be added **after** all of the following are true:

1. ✅ Synthetic harness produces clean, expected trends
2. ✅ At least one real OSS repo has full snapshot coverage (manually created)
3. ✅ Trend analysis is validated and worth operationalizing
4. ✅ Failure modes and edge cases are understood

**Current Phase:** Manual iteration is preferred for validation.

**Future Phase:** Once signal is validated, automation becomes justified.

**Estimated Effort (when ready):**
- Core implementation: ~2-3 hours
- Testing: ~1-2 hours
- Documentation: ~30 minutes

**Dependencies:**
- Uses existing `git.rs` and `snapshot.rs` modules
- No new dependencies required
- Can reuse `git_at()` pattern from `prune.rs`

---

## Related Features

Once this is implemented, it enables:

1. **Trend Analysis:** Process all commits, then analyze trends over time
2. **Historical Reports:** Generate reports showing complexity evolution
3. **Regression Detection:** Identify when complexity spiked
4. **Synthetic Harness:** Automate processing of synthetic repos

---

## Next Steps (Future)

**Do not implement now.** This is a parked design for future consideration.

When ready to implement:

1. **Add `history` subcommand** to CLI
2. **Create `history.rs` module** in `hotspots-core` (or add to existing module)
3. **Add tests** for batch processing
4. **Update documentation** with usage examples
5. **Consider adding progress bar** for long-running operations

**Current Focus:** Manual synthetic harness creation and validation.
