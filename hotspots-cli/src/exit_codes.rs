//! Named process exit codes, documented in `docs/REFERENCE.md`'s "Exit codes" table.
//! Keep that table in sync with this file when adding or changing a code.

/// `hotspots diff --policy` found a blocking policy violation.
pub(crate) const POLICY_VIOLATION: i32 = 1;

/// `hotspots analyze --validate-config` (or `config validate`) found an invalid config file.
/// Shares the same numeric value as [`POLICY_VIOLATION`] but is a distinct failure mode,
/// hence its own name.
pub(crate) const CONFIG_INVALID: i32 = 1;

/// `hotspots diff --auto-analyze` failed to generate a missing snapshot.
pub(crate) const AUTO_ANALYZE_FAILED: i32 = 2;

/// `hotspots diff` needs a snapshot that doesn't exist and `--auto-analyze` wasn't passed.
pub(crate) const SNAPSHOT_MISSING: i32 = 3;

/// `hotspots diff` failed for a reason other than a policy violation (bad ref,
/// config load failure, etc.) — kept distinct from [`POLICY_VIOLATION`] so CI
/// can tell "the code has a real problem" apart from "the tool itself failed to run."
pub(crate) const DIFF_TOOL_ERROR: i32 = 4;
