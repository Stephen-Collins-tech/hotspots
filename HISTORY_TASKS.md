# Historical Trend Analysis Tasks

**Goal:** Enable full historical trend analysis on any repo — bootstrap snapshot history from
git log, then visualize risk evolution over time as interactive charts and tables.

**Two features:**
1. `hotspots replay` — walks git history and generates snapshots for past commits
2. `hotspots trends --format html` — renders accumulated snapshots as an interactive report

---

## Feature 1: `hotspots replay`

### Overview

```
hotspots replay .
hotspots replay . --branch main --from 2024-01-01 --density daily
hotspots replay . --from abc1234 --to def5678 --dry-run
```

Walks the specified branch in chronological order. For each selected commit, spins up a
temporary `git worktree`, runs the existing analysis pipeline against it, persists the
snapshot, and tears down the worktree. Skips commits that already have snapshots (idempotent
and resumable — if interrupted, re-running picks up where it left off).

Progress output:
```
Replaying 347 commits on main (daily density → 89 selected)
  [  1/ 89] 2024-01-03  a3f91b2  feat: initial commit           ✓
  [  2/ 89] 2024-01-04  b82cd41  fix: null check in billing      ✓
  [  3/ 89] 2024-01-05  c91de3f  refactor: extract helpers       ✓ (skipped, exists)
  ...
  [ 89/ 89] 2026-02-16  3ce8b5c  feat: add --force flag          ✓
Done. 89 snapshots written to .hotspots/snapshots/
Run `hotspots trends . --format html` to visualize.
```

---

### R-1: Core Replay Engine

**Tasks:**

- [ ] **R-1a:** Add `Replay` variant to `Commands` enum in `hotspots-cli/src/main.rs`:
  ```rust
  Replay {
      path: PathBuf,
      #[arg(long)] branch: Option<String>,
      #[arg(long)] from: Option<String>,   // SHA or date (YYYY-MM-DD)
      #[arg(long)] to: Option<String>,
      #[arg(long, default_value = "every-commit")] density: ReplayDensity,
      #[arg(long)] skip: Option<usize>,    // analyze every Nth commit
      #[arg(long)] dry_run: bool,
      #[arg(short = 'f', long)] force: bool,
  }
  ```

- [ ] **R-1b:** Add `ReplayDensity` enum:
  ```rust
  enum ReplayDensity { EverCommit, Daily, Weekly }
  ```

- [ ] **R-1c:** Implement commit selection logic in `hotspots-core/src/git.rs`:
  - `list_commits_on_branch(repo_root, branch, from, to) -> Vec<CommitRef>`
  - Density filter: for `daily`/`weekly`, keep only the latest commit per day/week
  - Skip commits already in snapshot index (load index first)
  - Return list in chronological order (oldest first)

- [ ] **R-1d:** Implement worktree-based analysis in new `hotspots-core/src/replay.rs`:
  - `replay_commit(repo_root, commit_sha, config, force) -> Result<()>`
  - Create: `git worktree add --detach /tmp/hotspots-replay-<sha> <sha>`
  - Run: existing `analyze_with_config()` + `build_enriched_snapshot()` pipeline
  - Persist: `snapshot::persist_snapshot(repo_root, &snapshot, force)`
  - Append: `snapshot::append_to_index(repo_root, &snapshot)`
  - Cleanup: `git worktree remove --force /tmp/hotspots-replay-<sha>`
  - Error handling: if analysis fails for a commit, log warning and continue (don't abort the whole replay)

- [ ] **R-1e:** Implement progress display:
  - Print `[N/total] <date> <short-sha> <commit-subject>` with status (✓ / skipped / failed)
  - Flush stdout after each line so it streams in terminals

- [ ] **R-1f:** Wire up `Commands::Replay` in `main.rs` handler

**Acceptance:** `hotspots replay . --dry-run` lists commits that would be analyzed without
writing anything. `hotspots replay .` generates snapshots for all unanalyzed commits on the
default branch. Re-running is idempotent.

---

### R-2: Branch and Commit Resolution

**Tasks:**

- [ ] **R-2a:** Implement default branch detection:
  - Try `git symbolic-ref refs/remotes/origin/HEAD` → extract branch name
  - Fall back to checking for `main`, then `master`
  - Error clearly if neither exists and `--branch` not provided

- [ ] **R-2b:** Implement `--from` / `--to` resolution:
  - If value is a date (`YYYY-MM-DD`): resolve to earliest/latest commit on that date
  - If value is a SHA or ref: use directly via `git rev-parse`
  - Validate that `from` precedes `to` in history

- [ ] **R-2c:** Handle merge commits:
  - By default, walk only first-parent history (`git log --first-parent`) to follow mainline
  - Avoids re-analyzing feature branch commits merged in

**Acceptance:** `hotspots replay . --from 2024-01-01 --to 2024-06-30 --branch main` correctly
selects only commits on main between those dates.

---

### R-3: Safety and Edge Cases

**Tasks:**

- [ ] **R-3a:** Worktree cleanup on panic/interrupt — use a guard type (drop impl) so the
  worktree is always removed even if the process is killed mid-analysis.

- [ ] **R-3b:** Handle commits where analysis produces zero functions (empty repo, binary-only
  changes, etc.) — persist a valid but empty snapshot rather than failing.

- [ ] **R-3c:** Limit concurrent worktrees — since replay is sequential per commit, this is
  naturally bounded to 1 at a time. Document this and add a note for future parallelization.

- [ ] **R-3d:** Surface a summary at the end:
  ```
  Replay complete: 87 written, 2 skipped (existing), 0 failed
  Snapshot history: 2024-01-01 → 2026-02-16 (89 snapshots on main)
  ```

---

## Feature 2: `hotspots trends --format html`

### Overview

Renders the accumulated snapshot history as an interactive HTML report. Currently HTML format
for trends returns an error ("not supported"). This removes that limitation.

The report should answer:
- Which functions have been getting riskier over time?
- Which refactors actually worked and held?
- What does the risk band distribution look like over the past N snapshots?
- Who are the stable long-term hotspots that never got fixed?

---

### T-1: Trends HTML Report Structure

**Tasks:**

- [ ] **T-1a:** Design the report layout (sections):
  1. **Summary header** — date range, total snapshots, repo name
  2. **Risk band distribution over time** — stacked area chart (critical/high/moderate/low counts per snapshot)
  3. **Top hotspots table** — functions that appear most often in the top-K across all snapshots,
     with stability classification (stable / emerging / volatile) from `HotspotAnalysis`
  4. **Risk velocity table** — fastest-rising and fastest-falling functions from `RiskVelocity`
  5. **Refactor effectiveness table** — functions with significant improvements from `RefactorAnalysis`,
     showing outcome (Successful / Partial / Cosmetic) and whether a rebound was detected
  6. **Function detail (expandable)** — click a function to see its LRS over time as a line chart

- [ ] **T-1b:** Implement `TrendsAnalysis::to_html()` in `hotspots-core/src/trends.rs`:
  - Embed Chart.js via CDN (same approach as existing HTML report)
  - Inline the snapshot time-series data as a JSON blob in a `<script>` tag
  - Generate chart configs for each visualization
  - Return a complete, self-contained HTML string

- [ ] **T-1c:** Remove the error branch in `hotspots-cli/src/main.rs` for `Html` format on
  trends and wire up `to_html()` with output to `.hotspots/trends.html` (same pattern as
  `report.html` for snapshot mode). Respect `--output` flag for custom path.

**Acceptance:** `hotspots trends . --format html` produces a valid, self-contained HTML file
that renders correctly in a browser with no external dependencies other than Chart.js CDN.

---

### T-2: Time-Series Data for Charts

**Tasks:**

- [ ] **T-2a:** Extend `analyze_trends()` in `hotspots-core/src/trends.rs` to also return
  raw time-series data per snapshot (currently only returns aggregated velocity/hotspot/refactor
  summaries). Add `snapshots_summary: Vec<SnapshotPoint>` to `TrendsAnalysis`:
  ```rust
  pub struct SnapshotPoint {
      pub commit_sha: String,
      pub timestamp: i64,
      pub critical_count: usize,
      pub high_count: usize,
      pub moderate_count: usize,
      pub low_count: usize,
      pub total_functions: usize,
  }
  ```

- [ ] **T-2b:** For per-function line charts, collect LRS-over-time for the top-K functions.
  Add `function_series: Vec<FunctionSeries>` to `TrendsAnalysis`:
  ```rust
  pub struct FunctionSeries {
      pub function_id: String,
      pub points: Vec<(i64, f64)>,  // (timestamp, lrs)
  }
  ```
  Only include top-K functions (configurable, default 10) to keep HTML size reasonable.

**Acceptance:** `hotspots trends . --format json` output includes `snapshots_summary` and
`function_series` fields. Existing JSON consumers are unaffected (additive fields).

---

### T-3: Polish

- [ ] **T-3a:** Add `--top N` flag to `hotspots trends` to control how many functions appear
  in velocity/hotspot/refactor tables and how many get individual line charts (currently
  `top` only controls hotspot analysis K).

- [ ] **T-3b:** Add `--output <path>` flag to `hotspots trends` for custom HTML output path.

- [ ] **T-3c:** Update `hotspots trends --help` text to accurately describe all three analyses
  (velocity, hotspot stability, refactor effectiveness) and the available output formats.

---

## Suggested Order of Attack

```
R-1c  →  R-2a, R-2b  →  R-1d  →  R-1a, R-1b  →  R-1e, R-1f  →  R-2c  →  R-3a–R-3d
T-2a  →  T-2b  →  T-1b  →  T-1a  →  T-1c  →  T-3a–T-3c
```

Replay (R) and trends HTML (T) are independent — can be worked in parallel.
Start with R-1c and T-2a as they are the core logic with no UI dependencies.

---

## Out of Scope (for now)

- Parallel replay (multiple worktrees at once) — correctness first
- Replay of non-linear history (feature branches, forks)
- Streaming the HTML chart as snapshots are replayed in real time
- Exporting chart data to CSV/parquet
- `hotspots trends --format jsonl`

---

**Created:** 2026-02-16
**Branch:** improve/architecture
