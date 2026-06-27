# REQ-002 — convention_bug_fix_count as Ranker Feature

**Source finding:** F54 — Bug-Commits Temporal Holdout  
**Research brief:** `hotspots-research/docs/promotion-briefs/f54-convention-bug-fix-feature.md`  
**Status:** ready to implement  

---

## Problem

The current 9-feature RandomForest ranker has no signal for bug-fix commit frequency.
`total_churn` counts all commits; it cannot distinguish a file touched 200 times in
feature work from one touched 200 times in bug fixes. `convention_bug_fix_count` — the
count of commits whose message matches fix-keyword conventions — is a direct proxy for
historical defect concentration at the function level.

## What to build

Add `convention_bug_fix_count` as a 10th feature to the `FEATURE_NAMES` array and
`extract_features` function in `trainer.rs`. The field does not yet exist on
`FunctionSnapshot` — it must be added to `snapshot.rs` along with a
`populate_convention_bug_fix_count` method that counts fix-keyword commits per file
from git history. No new CLI flags. No user-visible change except the model version bump.

## Evidence

F54 temporal holdout (features pre-2024-01-01, labels post-2024-01-01):

| Repo | ρ | ARS ρ | Verdict |
|---|---|---|---|
| django | +0.318 | +0.140 | SIGNAL |
| vscode | +0.232 | +0.056 | SIGNAL |
| vuejs | +0.572 | +0.104 | SIGNAL |
| frp | +0.191 | — | SIGNAL |
| httpx | +0.264 | — | SIGNAL |
| gin | +0.504 | — | SIGNAL |
| axios | +0.286 | — | SIGNAL |
| golang | +0.166 | +0.229 | WEAK |
| react | +0.001 | +0.249 | CIRCULAR |

**7/9 SIGNAL, 1 WEAK, 1 CIRCULAR. Mean ρ=+0.269 vs ARS +0.156.**

Clone-only signal — no GitHub API required. Available in all deployment contexts.

---

## Files to change

`hotspots-core/src/snapshot.rs` and `hotspots-core/src/trainer.rs`:

```rust
// Before:
pub const FEATURE_NAMES: [&str; 9] = [
    "lrs", "cc", "nd", "loc", "fo",
    "fan_in", "total_churn", "authors_90d", "directed_coupling",
];

pub fn extract_features(func: &FunctionSnapshot) -> [f64; 9] {
    // ... existing 9 values ...
    [lrs, cc, nd, loc, fo, fan_in, total_churn, authors_90d, directed_coupling]
}

// After:
pub const FEATURE_NAMES: [&str; 10] = [
    "lrs", "cc", "nd", "loc", "fo",
    "fan_in", "total_churn", "authors_90d", "directed_coupling",
    "convention_bug_fix_count",
];

pub fn extract_features(func: &FunctionSnapshot) -> [f64; 10] {
    // ... existing 9 values, then: ...
    let convention_bug_fix_count = func
        .convention_bug_fix_count
        .unwrap_or(0) as f64;
    [lrs, cc, nd, loc, fo, fan_in, total_churn, authors_90d, directed_coupling,
     convention_bug_fix_count]
}
```

Also in `trainer.rs`:
- Bump `model_version` constant by 1
- Update `feature_names_count_matches_array` test: `assert_eq!(FEATURE_NAMES.len(), 10)`
- Add: `assert!(FEATURE_NAMES.contains(&"convention_bug_fix_count"))`

**Verify** the exact field name on `FunctionSnapshot` before editing — it may be
`convention_bug_fix_count: Option<u32>` or similar. Check `snapshot.rs`.

---

## Exact names

| Thing | Value |
|---|---|
| Feature name string | `"convention_bug_fix_count"` |
| Array length | `10` |
| Fallback for `None` | `0` |
| `model_version` | current + 1 (check before editing) |

---

## Acceptance criteria

1. `cargo test` passes in `hotspots-core` and `hotspots-cli`.
2. `FEATURE_NAMES.len() == 10`.
3. `extract_features` returns `[f64; 10]`.
4. `hotspots train` completes without panic on any previously-working repo.
5. `ranker.json` `model_version` is incremented.
6. A `ranker.json` produced by the previous binary (9 features) is not silently applied
   to new snapshots — the existing version-check logic should reject it and fall back to
   LRS-only scoring. Verify this happens.
7. `hotspots analyze` on a repo without a retrained ranker still works (falls back to
   activity risk score as before).

---

## Do not

- Do not add `convention_bug_fix_rate` — the rate is circular per F48; only the count survives.
- Do not add a CLI flag or any user-visible output change.
- Do not change `extract_features` for any existing feature.
- Do not add this feature to the explanation phrase table — it is not in scope for REQ-003.

---

## Supporting evidence

- `hotspots-research/docs/findings/54-bug-commits-temporal-holdout.md`
- `hotspots-research/docs/findings/47-heuristic-signal-eval.md` (in-sample ρ reference)
- `hotspots-research/docs/findings/48-fix-density-temporal-holdout.md` (why rate fails)
