# Training a Ranker

By default, Hotspots ranks functions by **LRS** — a structural complexity score derived from the code itself. This works out of the box on any repo.

`hotspots train` fits a local RandomForest model against your repo's own git history, learning which structural features correlate with bug-fix commits in *your* codebase. Once trained, `hotspots analyze` picks up the model automatically and uses it to re-rank functions and update triage quadrants.

---

## When it's worth training

Training is most valuable when:

- Your repo has **at least a year of meaningful commit history**
- Fix commits follow recognizable conventions (`fix:`, `bug`, `patch`, `hotfix`, `regression`, `defect`)
- Bugs have been concentrated in specific files or functions rather than spread uniformly

Training is unlikely to help on:

- Library or framework repos with low bug rates and tiny base rates
- Repos with less than ~1 year of history
- Repos with poor commit hygiene (unflagged fix commits can't be scanned)
- Stable mature codebases where bugs are subtle and spread across the whole codebase

Use `--eval` (described below) to verify the model actually learned something before relying on it.

---

## Basic usage

```bash
# Train with file-level labels (fast)
hotspots train .

# Train with blame-based function-level labels (slower, more precise)
hotspots train . --blame
```

The model is saved to `.hotspots/ranker.json`. Once it exists, `hotspots analyze` loads it automatically on every run.

---

## Label modes

### File-level (default)

Every function in a file touched by a fix commit is marked as a positive training example. Fast, but noisy — a bug in one function marks every function in that file.

### Blame-based (`--blame`)

`git diff-tree` hunk headers are parsed to identify which exact function owned the changed lines. Only that function is marked positive.

More precise signal, but requires one subprocess per fix commit. Recommended for repos with large files where a single file can contain many unrelated functions.

```bash
# Recommended for most production repos
hotspots train . --blame
```

---

## Checking whether training helped (`--eval`)

`hotspots train` always produces a model, but that doesn't mean the model is better than the default LRS ranking. `--eval` measures this by computing **Precision@K** — how many of the top K ranked functions actually appeared in a bug-fix commit:

```bash
hotspots train . --blame --eval
```

Example output:

```
P@K evaluation (365-day fix-label window):
  K      P@K      base_rate
  10     0.400    0.084
  20     0.300    0.084
  50     0.200    0.084
  100    0.150    0.084
  200    0.110    0.084
```

**How to read it:**

- `base_rate` is the fraction of all functions that appeared in any fix commit. It's the score a random ranker would achieve.
- If `P@K` is meaningfully above `base_rate` (especially at low K), the model is surfacing real bug-prone functions at the top of the list.
- If `P@K ≈ base_rate`, the model learned nothing useful. In that case, stick with the default LRS ranking — applying a weak model can demote functions that are genuinely risky.

---

## Label window

By default, the scanner looks back 365 days for fix commits. Adjust with `--label-window`:

```bash
# Use 6 months of history
hotspots train . --blame --label-window 180

# Use 2 years for repos with sparse bug fixes
hotspots train . --blame --label-window 730
```

Larger windows provide more training examples but may include stale patterns that no longer reflect the codebase.

---

## What the model trains on

The v3 model trains on 8 structural features: `lrs`, `cc`, `nd`, `loc`, `fo`, `fan_in`, `total_churn`, `authors_90d`.

Windowed activity signals (`touch_count_30d`, `days_since_last_change`, `activity_risk`) are deliberately excluded to prevent temporal leakage — these signals would be computed from the same time window being used for labels, inflating apparent performance.

---

## How it integrates with `hotspots analyze`

Once `.hotspots/ranker.json` exists, every `hotspots analyze` run automatically:

1. Loads the ranker and scores every function
2. Uses RF scores to determine triage quadrants (in place of the activity heuristic)
3. Promotes high-probability functions from `debt` → `fire` where the model score warrants it

A suppression gate runs on every analysis to detect poor model performance. If `P@10` is at or below the base rate, the output will note that the model is not performing above baseline and suggest re-training or relying on the default LRS ranking.

---

## Minimum requirements

Training will return an error if:

- The snapshot has fewer than **50 functions**, or
- The fix scan yields fewer than **5 positive** or **10 negative** labels

If this happens:

- Try a larger `--label-window`
- Run `hotspots analyze . --mode snapshot --force` to regenerate a fresh snapshot
- Verify fix commits use recognizable keywords (`fix:`, `bug`, `patch`, `hotfix`, `regression`, `defect`)

---

## Re-training

The model reflects the git history at the time it was trained. Re-train periodically as new fix commits accumulate:

```bash
# Re-train and immediately re-analyze
hotspots train . --blame && hotspots analyze .
```

There is no automatic re-training — the model file is updated only when you explicitly run `hotspots train`.

---

## Full options reference

See [`hotspots train`](/reference/cli#hotspots-train) in the CLI reference for all flags and defaults.
