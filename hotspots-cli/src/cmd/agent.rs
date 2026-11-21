//! `hotspots agent` — a dense, machine-readable contract for coding agents driving
//! `hotspots` non-interactively. Hand-written, not derived from the Clap command tree:
//! validate the shape manually first, consider codegen later.

#[derive(clap::Subcommand)]
pub(crate) enum AgentAction {
    /// Print the full contract (analyze + diff)
    All,
    /// Contract scoped to `hotspots analyze`
    Analyze,
    /// Contract scoped to `hotspots diff`
    Diff,
}

const EXECUTION_CONTRACT: &str = "\
## Execution Contract

- Always pass `--format json`. Human-readable `text`/`html` output is for people, not parsers.
- Neither `analyze` nor `diff` prompts interactively. The one exception in the whole CLI is
  `hotspots train`: it shows a confirmation prompt on repos with more than 1000 functions. Pass
  `--yes` to `train` when driving it non-interactively.
- stdout carries the data (the JSON document). stderr carries diagnostics, progress lines, and
  warnings — never parse stderr for results, and don't treat its presence as failure.
- Exit code is the pass/fail signal; always check it, don't infer success from stdout being
  non-empty.
";

const ANALYZE_SCHEMA_MAP: &str = "\
## Command & Schema Map — analyze

    hotspots analyze <PATH> --mode snapshot --format json [OPTIONS]

Flags (see docs/REFERENCE.md \"hotspots analyze <path>\" for the full table and defaults):
  --top N            limit output to top N functions by LRS
  --min-lrs F         filter functions below this LRS
  --config PATH       config file (default: auto-discover)
  --all-functions     flat array instead of triage buckets (snapshot JSON only)
  --explain           per-function risk breakdown (requires --level or non-json text — not
                      needed with --format json, which always includes full fields)
  --level file        ranked file risk table instead of per-function (text format only)
  --no-persist        skip writing the snapshot to disk
  --force / -f        overwrite an existing snapshot
  --skip-touch-metrics  skip git-history I/O (churn/recency/coupling/burst) entirely
  --cold-start        Gini-gated ranking strategy for repos with no fix-commit label history
  --jobs N / -j N     parallel worker threads

Mode is `snapshot` only — there is no other `--mode` value. `analyze` has no policy-evaluation
mode; use `hotspots diff --policy` to gate a change.

JSON shape: schema_version, functions[] (with --all-functions) or triage{active,quiet} buckets,
aggregates (file-level rollups), suppression gate verdict. Full field-by-field shape, including
`file_risk_score`/`file_risk_diff_percentile` and tier-stability caveats, is in
docs/REFERENCE.md under \"hotspots analyze <path>\" and \"The four structural metrics\" — read
that rather than guessing field names from a single sample run.
";

const DIFF_SCHEMA_MAP: &str = "\
## Command & Schema Map — diff

    hotspots diff [BASE] [HEAD] --format json [OPTIONS]

`BASE` and `HEAD` are git refs (branch, tag, SHA, `HEAD~N`). Omit both to auto-detect `HEAD`
against the auto-detected merge-base. This is the PR-review entry point — prefer it over
`analyze` for \"what did this change do to risk\".

Flags (see docs/REFERENCE.md \"hotspots diff [base] [head]\" for the full table):
  --policy            evaluate policies; exit 1 on blocking violations
  --top N             limit to top N new/modified functions by LRS / |ΔLRS|; deleted functions
                      never count toward N
  --config PATH       config file
  --auto-analyze      generate missing snapshots via temporary git worktrees
  --scope-to-diff     with --auto-analyze, only analyze files the diff touches (50-70x cheaper
                      on large repos with small PRs — see docs/REFERENCE.md \"--scope-to-diff\"
                      before relying on it for a full-repo-accurate result; it is opt-in because
                      cross-file call-graph effects outside the scoped set are not accounted for)
  --explain           one-line factor breakdown per changed function

JSON shape: schema_version, delta{deltas[], policy{failed,warnings}, aggregates}. Each delta
entry has status (new/modified/deleted), before/after LRS and band. `aggregates.files[]` carries
`file_risk_top_decile`/`file_risk_score`/`file_risk_diff_percentile`, a diff-local ranking (only
against the other files changed in *this* diff, not the whole repo). Full shape in
docs/REFERENCE.md under \"hotspots diff [base] [head]\".
";

const EXIT_CODES: &str = "\
## Exit Code Semantics

  0   success (including policy warnings only, if any)
  1   error, or a blocking policy failure from --policy
  2   auto-analysis failed (diff --auto-analyze only)
  3   a required snapshot is missing (diff only, without --auto-analyze)

Full table and the exit-4 case `analyze --mode delta --policy` used to occupy (removed along
with that mode) is in docs/REFERENCE.md \"Exit codes\". Codes 2 and 3 are diff-specific; analyze
only ever returns 0 or 1.
";

const GUARDRAILS: &str = "\
## Guardrails & Anti-Patterns

- Don't parse the `text` table output. Column widths and wrapping are for terminals, not
  programs — use `--format json` and read the documented fields.
- `analyze --mode delta` and `analyze --policy` do not exist. Use `hotspots diff --policy`
  instead for any \"what did this change break\" question.
- `diff --auto-analyze` without `--scope-to-diff` walks and parses the entire repo to build a
  missing snapshot, even for a one-file PR — see the benchmark numbers in docs/REFERENCE.md
  under \"--scope-to-diff\" before defaulting to the unscoped form on a large repo.
- `tier` in analyze's output is population-relative and can change run-to-run even when a
  function's own lrs/activity_risk/band are unchanged — don't key a CI gate on it; see
  docs/REFERENCE.md \"Field stability\".
- `train`'s confirmation prompt only appears above 1000 functions; pass `--yes` for
  non-interactive runs rather than assuming it never prompts.
";

fn print_analyze_contract() {
    print!("{EXECUTION_CONTRACT}\n{ANALYZE_SCHEMA_MAP}\n{EXIT_CODES}\n{GUARDRAILS}");
}

fn print_diff_contract() {
    print!("{EXECUTION_CONTRACT}\n{DIFF_SCHEMA_MAP}\n{EXIT_CODES}\n{GUARDRAILS}");
}

fn print_all_contract() {
    print!(
        "{EXECUTION_CONTRACT}\n{ANALYZE_SCHEMA_MAP}\n{DIFF_SCHEMA_MAP}\n{EXIT_CODES}\n{GUARDRAILS}"
    );
}

pub(crate) fn handle_agent(action: AgentAction) {
    match action {
        AgentAction::All => print_all_contract(),
        AgentAction::Analyze => print_analyze_contract(),
        AgentAction::Diff => print_diff_contract(),
    }
}
