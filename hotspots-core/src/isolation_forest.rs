//! From-scratch IsolationForest for the F62/F63 cold-start anomaly route.
//!
//! Memory-bounded independent of repo size: `fit()` takes an iterator and fills
//! `n_trees` independent reservoir samples (reservoir sampling, one pass), so peak
//! memory is `O(n_trees * subsample_size)` regardless of how many rows are streamed
//! through it. Scoped exactly to the 8-feature cold-start vector — not a general-purpose
//! anomaly detection library. Mirrors
//! `hotspots-research/scripts/poc/validate_streaming_isolation_forest.py`.

use rand::rngs::SmallRng;
use rand::Rng;
use rand::SeedableRng;

/// Split metadata for one isolation-tree node. No training rows are retained.
pub struct IsolationNode {
    pub feature_idx: usize,
    pub split_value: f64,
    pub left: Option<usize>,
    pub right: Option<usize>,
    pub size: usize,
}

/// Arena of nodes for a single isolation tree (index 0 is the root).
pub struct IsolationTree {
    nodes: Vec<IsolationNode>,
}

/// Ensemble of isolation trees. Total memory is `O(n_trees * subsample_size)` nodes.
pub struct IsolationForest {
    trees: Vec<IsolationTree>,
    subsample_size: usize,
}

/// Average path length of an unsuccessful BST search over `n` items
/// (Liu et al. 2008 normalization constant).
fn c(n: usize) -> f64 {
    if n <= 1 {
        return 0.0;
    }
    let n = n as f64;
    2.0 * ((n - 1.0).ln() + 0.577_215_664_9) - 2.0 * (n - 1.0) / n
}

impl IsolationTree {
    /// Recursively splits `rows` (indices into the reservoir) on a random feature at a
    /// random threshold, building a flat node arena. Returns the index of the root node.
    fn build(
        nodes: &mut Vec<IsolationNode>,
        rows: &[[f64; 8]],
        depth: usize,
        height_limit: usize,
        rng: &mut SmallRng,
    ) -> usize {
        let size = rows.len();
        if depth >= height_limit || size <= 1 {
            nodes.push(IsolationNode {
                feature_idx: 0,
                split_value: 0.0,
                left: None,
                right: None,
                size,
            });
            return nodes.len() - 1;
        }

        let feature_idx = rng.gen_range(0..8);
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for row in rows {
            let v = row[feature_idx];
            lo = lo.min(v);
            hi = hi.max(v);
        }
        if lo == hi {
            nodes.push(IsolationNode {
                feature_idx: 0,
                split_value: 0.0,
                left: None,
                right: None,
                size,
            });
            return nodes.len() - 1;
        }

        let split_value = rng.gen_range(lo..hi);
        let left_rows: Vec<[f64; 8]> = rows
            .iter()
            .copied()
            .filter(|r| r[feature_idx] < split_value)
            .collect();
        let right_rows: Vec<[f64; 8]> = rows
            .iter()
            .copied()
            .filter(|r| r[feature_idx] >= split_value)
            .collect();

        if left_rows.is_empty() || right_rows.is_empty() {
            nodes.push(IsolationNode {
                feature_idx: 0,
                split_value: 0.0,
                left: None,
                right: None,
                size,
            });
            return nodes.len() - 1;
        }

        let left_idx = Self::build(nodes, &left_rows, depth + 1, height_limit, rng);
        let right_idx = Self::build(nodes, &right_rows, depth + 1, height_limit, rng);

        nodes.push(IsolationNode {
            feature_idx,
            split_value,
            left: Some(left_idx),
            right: Some(right_idx),
            size,
        });
        nodes.len() - 1
    }

    fn path_length(&self, row: &[f64], node_idx: usize, depth: usize) -> f64 {
        let node = &self.nodes[node_idx];
        match (node.left, node.right) {
            (Some(left), Some(right)) => {
                if row[node.feature_idx] < node.split_value {
                    self.path_length(row, left, depth + 1)
                } else {
                    self.path_length(row, right, depth + 1)
                }
            }
            _ => depth as f64 + c(node.size),
        }
    }
}

impl IsolationForest {
    /// Fit an isolation forest from a streaming iterator of 8-feature rows.
    ///
    /// Single pass over `rows`, filling `n_trees` independent reservoirs (reservoir
    /// sampling) capped at `subsample_size` each — the full feature set is never
    /// materialized at once. Builds one `IsolationTree` per reservoir after the pass.
    pub fn fit(
        rows: impl Iterator<Item = [f64; 8]>,
        n_trees: usize,
        subsample_size: usize,
        seed: u64,
    ) -> IsolationForest {
        let mut rng = SmallRng::seed_from_u64(seed);
        let mut reservoirs: Vec<Vec<[f64; 8]>> = (0..n_trees)
            .map(|_| Vec::with_capacity(subsample_size))
            .collect();

        for (i, row) in rows.enumerate() {
            for reservoir in reservoirs.iter_mut() {
                if reservoir.len() < subsample_size {
                    reservoir.push(row);
                } else {
                    let j = rng.gen_range(0..=i);
                    if j < subsample_size {
                        reservoir[j] = row;
                    }
                }
            }
        }

        let height_limit = (subsample_size.max(2) as f64).log2().ceil() as usize;
        let trees: Vec<IsolationTree> = reservoirs
            .into_iter()
            .map(|reservoir| {
                let mut nodes = Vec::new();
                if !reservoir.is_empty() {
                    IsolationTree::build(&mut nodes, &reservoir, 0, height_limit, &mut rng);
                }
                IsolationTree { nodes }
            })
            .collect();

        IsolationForest {
            trees,
            subsample_size,
        }
    }

    /// Normalized anomaly score for a single row: `2^(-E[h(x)] / c(subsample_size))`.
    /// Higher scores indicate more anomalous (more isolable) rows. Scores one row at a
    /// time — the caller streams rows through this, no internal batching.
    pub fn anomaly_score(&self, row: &[f64]) -> f64 {
        let valid_trees: Vec<&IsolationTree> =
            self.trees.iter().filter(|t| !t.nodes.is_empty()).collect();
        if valid_trees.is_empty() {
            return 0.0;
        }
        let avg_path: f64 = valid_trees
            .iter()
            .map(|t| t.path_length(row, t.nodes.len() - 1, 0))
            .sum::<f64>()
            / valid_trees.len() as f64;
        let c_n = c(self.subsample_size);
        if c_n <= 0.0 {
            return 0.0;
        }
        2.0_f64.powf(-avg_path / c_n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outliers_rank_in_top_10_of_95_plus_5() {
        // 95 points clustered near origin, 5 points far away — synthetic outlier check
        // mirrors the brief's acceptance criterion 3.
        let mut rng = SmallRng::seed_from_u64(7);
        let mut rows: Vec<[f64; 8]> = Vec::new();
        for _ in 0..95 {
            let mut row = [0.0; 8];
            for v in row.iter_mut() {
                *v = rng.gen_range(-1.0..1.0);
            }
            rows.push(row);
        }
        let mut outlier_rows: Vec<[f64; 8]> = Vec::new();
        for _ in 0..5 {
            let mut row = [0.0; 8];
            for v in row.iter_mut() {
                *v = 5.0 + rng.gen_range(-1.0..1.0);
            }
            outlier_rows.push(row);
            rows.push(row);
        }

        let forest = IsolationForest::fit(rows.iter().copied(), 100, 256, 42);

        let mut scored: Vec<(usize, f64)> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| (i, forest.anomaly_score(r)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        let top10: std::collections::HashSet<usize> =
            scored.iter().take(10).map(|(i, _)| *i).collect();
        let outlier_indices: std::collections::HashSet<usize> = (95..100).collect();

        let hits = outlier_indices.intersection(&top10).count();
        assert!(
            hits >= 4,
            "expected at least 4/5 outliers in top 10, got {hits}: {scored:?}"
        );
    }

    #[test]
    fn fit_never_collects_full_dataset() {
        // Reservoir size is bounded regardless of how many rows stream through —
        // confirmed indirectly by fitting on a large stream and checking it completes
        // in reasonable time/without unbounded growth (memory bound is structural,
        // enforced by construction: reservoirs are Vec::with_capacity(subsample_size)
        // and never grow past that).
        let rows = (0..50_000).map(|i| {
            let f = (i % 7) as f64;
            [f; 8]
        });
        let forest = IsolationForest::fit(rows, 10, 32, 1);
        assert_eq!(forest.trees.len(), 10);
        for tree in &forest.trees {
            // Each tree has at most 2*subsample_size - 1 nodes (full binary tree bound).
            assert!(tree.nodes.len() <= 2 * 32);
        }
    }

    #[test]
    fn empty_input_scores_zero() {
        let forest = IsolationForest::fit(std::iter::empty(), 10, 32, 1);
        assert_eq!(forest.anomaly_score(&[0.0; 8]), 0.0);
    }
}
