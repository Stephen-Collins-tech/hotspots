//! Views - projections of `Snapshot`/`FunctionSnapshot` for specific consumers.
//!
//! A view selects, renames, bounds, or renders fields from the master snapshot;
//! it never recomputes a metric. Metric computation belongs in `analysis.rs`.

pub mod agent;
