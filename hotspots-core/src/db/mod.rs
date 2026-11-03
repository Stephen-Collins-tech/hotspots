//! SQLite-backed snapshot storage.
//!
//! Provides an alternative to per-commit `.json.zst` files: a single
//! `.hotspots/snapshots.db` that stores all function rows and allows SQL
//! queries across commits.
//!
//! The module exposes two types:
//! - [`SnapshotDb`]: persistent SQLite database (`.hotspots/snapshots.db`)
//! - [`TempDb`]: in-memory SQLite database for intermediate use during analysis
//!
//! Both types use the same schema and share insert/query helpers.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

use crate::report::FunctionRiskReport;
use crate::snapshot::{CallGraphMetrics, ChurnMetrics, CommitInfo, FunctionSnapshot, Snapshot};

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS commits (
    sha             TEXT    PRIMARY KEY,
    timestamp       INTEGER NOT NULL,
    branch          TEXT,
    message         TEXT,
    author          TEXT,
    is_fix_commit   INTEGER,
    is_revert_commit INTEGER,
    ticket_ids      TEXT,
    parents         TEXT
);

CREATE TABLE IF NOT EXISTS functions (
    id                      INTEGER PRIMARY KEY,
    commit_sha              TEXT    NOT NULL,
    function_id             TEXT    NOT NULL,
    file                    TEXT    NOT NULL,
    line                    INTEGER NOT NULL,
    language                TEXT    NOT NULL,
    cc                      INTEGER NOT NULL,
    nd                      INTEGER NOT NULL,
    fo                      INTEGER NOT NULL,
    ns                      INTEGER NOT NULL,
    loc                     INTEGER NOT NULL,
    lrs                     REAL    NOT NULL,
    band                    TEXT    NOT NULL,
    suppression_reason      TEXT,
    callees                 TEXT,
    churn_added             INTEGER,
    churn_deleted           INTEGER,
    touch_count         INTEGER,
    days_since_last_change  INTEGER,
    fan_in                  INTEGER,
    activity_risk           REAL,
    risk_factors            TEXT,
    tier                    TEXT,
    FOREIGN KEY (commit_sha) REFERENCES commits(sha),
    UNIQUE (commit_sha, function_id)
);

CREATE INDEX IF NOT EXISTS idx_functions_commit ON functions(commit_sha);
CREATE INDEX IF NOT EXISTS idx_functions_band   ON functions(commit_sha, band);
CREATE INDEX IF NOT EXISTS idx_functions_lrs    ON functions(commit_sha, lrs DESC);
"#;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Apply the schema DDL to an open connection.
fn apply_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA).context("failed to apply schema")
}

/// Insert a commit row, ignoring conflicts (idempotent).
fn insert_commit(conn: &Connection, commit: &CommitInfo) -> Result<()> {
    let ticket_ids = serde_json::to_string(&commit.ticket_ids)?;
    let parents = serde_json::to_string(&commit.parents)?;
    conn.execute(
        "INSERT OR REPLACE INTO commits
            (sha, timestamp, branch, message, author,
             is_fix_commit, is_revert_commit, ticket_ids, parents)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            commit.sha,
            commit.timestamp,
            commit.branch,
            commit.message,
            commit.author,
            commit.is_fix_commit.map(|b| b as i64),
            commit.is_revert_commit.map(|b| b as i64),
            ticket_ids,
            parents,
        ],
    )
    .context("failed to insert commit")?;
    Ok(())
}

/// Insert all functions from a snapshot into the given connection.
/// Uses a single transaction for performance.
fn insert_functions(conn: &Connection, snapshot: &Snapshot) -> Result<()> {
    let sha = &snapshot.commit.sha;
    let mut stmt = conn.prepare(
        "INSERT OR REPLACE INTO functions (
            commit_sha, function_id, file, line, language,
            cc, nd, fo, ns, loc, lrs, band, suppression_reason,
            churn_added, churn_deleted,
            touch_count, days_since_last_change,
            fan_in,
            activity_risk, risk_factors,
            tier
        ) VALUES (
            ?1,?2,?3,?4,?5,
            ?6,?7,?8,?9,?10,?11,?12,?13,
            ?14,?15,
            ?16,?17,
            ?18,
            ?19,?20,
            ?21
        )",
    )?;

    for func in &snapshot.functions {
        let risk_factors_json = func
            .risk_factors
            .as_ref()
            .and_then(|rf| serde_json::to_string(rf).ok());

        let (churn_added, churn_deleted) = func
            .churn
            .as_ref()
            .map(|c| (Some(c.lines_added as i64), Some(c.lines_deleted as i64)))
            .unwrap_or((None, None));

        let fan_in = func.callgraph.as_ref().map(|cg| cg.fan_in as i64);

        stmt.execute(params![
            sha,
            func.function_id,
            func.file,
            func.line as i64,
            func.language.name(),
            func.metrics.cc as i64,
            func.metrics.nd as i64,
            func.metrics.fo as i64,
            func.metrics.ns as i64,
            func.metrics.loc as i64,
            func.lrs,
            func.band.as_str(),
            func.suppression_reason,
            churn_added,
            churn_deleted,
            func.touch_count.map(|n| n as i64),
            func.days_since_last_change.map(|n| n as i64),
            fan_in,
            func.activity_risk,
            risk_factors_json,
            func.tier,
        ])
        .context("failed to insert function row")?;
    }

    Ok(())
}

/// Load all function rows for a commit SHA into a Vec, ordered by function_id.
fn load_functions(conn: &Connection, sha: &str) -> Result<Vec<FunctionSnapshot>> {
    use crate::report::MetricsReport;

    let mut stmt = conn.prepare(
        "SELECT function_id, file, line, language,
                cc, nd, fo, ns, loc, lrs, band, suppression_reason,
                churn_added, churn_deleted,
                touch_count, days_since_last_change,
                fan_in,
                activity_risk, risk_factors,
                tier
         FROM functions
         WHERE commit_sha = ?1
         ORDER BY function_id",
    )?;

    let rows = stmt.query_map([sha], |row| {
        let function_id: String = row.get(0)?;
        let file: String = row.get(1)?;
        let line: i64 = row.get(2)?;
        let language: String = row.get(3)?;
        let cc: i64 = row.get(4)?;
        let nd: i64 = row.get(5)?;
        let fo: i64 = row.get(6)?;
        let ns: i64 = row.get(7)?;
        let loc: i64 = row.get(8)?;
        let lrs: f64 = row.get(9)?;
        let band: String = row.get(10)?;
        let suppression_reason: Option<String> = row.get(11)?;

        let churn_added: Option<i64> = row.get(12)?;
        let churn_deleted: Option<i64> = row.get(13)?;
        let churn = churn_added.zip(churn_deleted).map(|(a, d)| ChurnMetrics {
            lines_added: a as usize,
            lines_deleted: d as usize,
        });

        let touch_count: Option<i64> = row.get(14)?;
        let days_since_last_change: Option<i64> = row.get(15)?;

        let fan_in: Option<i64> = row.get(16)?;
        let callgraph = fan_in.map(|fi| CallGraphMetrics {
            fan_in: fi as usize,
        });

        let activity_risk: Option<f64> = row.get(17)?;
        let risk_factors_json: Option<String> = row.get(18)?;
        let tier: Option<String> = row.get(19)?;

        Ok((
            function_id,
            file,
            line,
            language,
            cc,
            nd,
            fo,
            ns,
            loc,
            lrs,
            band,
            suppression_reason,
            churn,
            touch_count,
            days_since_last_change,
            callgraph,
            activity_risk,
            risk_factors_json,
            tier,
        ))
    })?;

    let mut functions = Vec::new();
    for row in rows {
        let (
            function_id,
            file,
            line,
            language,
            cc,
            nd,
            fo,
            ns,
            loc,
            lrs,
            band,
            suppression_reason,
            churn,
            touch_count,
            days_since_last_change,
            callgraph,
            activity_risk,
            risk_factors_json,
            tier,
        ) = row.context("failed to read function row")?;

        let risk_factors = risk_factors_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());

        let language = crate::language::Language::from_name(&language)
            .unwrap_or(crate::language::Language::TypeScript);
        let band = crate::risk::RiskBand::parse(&band).unwrap_or(crate::risk::RiskBand::Low);
        functions.push(FunctionSnapshot {
            function_id,
            file,
            line: line as u32,
            language,
            metrics: MetricsReport {
                cc: cc as u32,
                nd: nd as u32,
                fo: fo as u32,
                ns: ns as u32,
                loc: loc as u32,
                // Not persisted in the DB cache schema; this is an
                // `--explain`-layer diagnostic recomputed fresh each run,
                // not a stored/scored value.
                missing_docstring: false,
            },
            lrs,
            band,
            suppression_reason,
            churn,
            touch_count: touch_count.map(|n| n as usize),
            days_since_last_change: days_since_last_change.map(|n| n as u32),
            callgraph,
            activity_risk,
            risk_factors,
            tier,
            subsystem: None,
            authors_90d: None,
            directed_coupling: None,
            convention_bug_fix_count: None,
            burst_score: None,
            commit_count: None,
            author_count: None,
            author_entropy: None,
            isolation_rate: None,
            age_days: None,
            last_touch_days: None,
            newcomer_rate: None,
            explanation: None,
        });
    }

    Ok(functions)
}

// ---------------------------------------------------------------------------
// TempDb — in-memory database for a single analysis run
// ---------------------------------------------------------------------------

/// Temporary in-memory SQLite database.
///
/// Created for a single analysis run, holds function rows during output
/// generation, and is dropped when the run completes.
pub struct TempDb {
    conn: Connection,
}

impl TempDb {
    /// Create a new in-memory database with the hotspots schema applied.
    pub fn new() -> Result<Self> {
        let conn = Connection::open_in_memory().context("failed to open in-memory SQLite")?;
        apply_schema(&conn)?;
        Ok(TempDb { conn })
    }

    /// Insert commit metadata and raw analysis reports into the pipeline buffer.
    ///
    /// Writes one row per report with enrichment columns (churn, touch, call graph,
    /// activity_risk, etc.) all NULL. Subsequent pipeline phases fill them in via
    /// SQL UPDATE. Drops the caller's `reports` Vec after this returns to free ~23 MB.
    pub fn insert_reports(
        &self,
        commit: &CommitInfo,
        reports: &[FunctionRiskReport],
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        insert_commit(&self.conn, commit)?;

        let sha = &commit.sha;
        let mut stmt = self.conn.prepare(
            "INSERT OR REPLACE INTO functions (
                commit_sha, function_id, file, line, language,
                cc, nd, fo, ns, loc, lrs, band, suppression_reason, callees
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
        )?;

        for report in reports {
            let normalized_file = report.file.replace('\\', "/");
            let function_symbol = if report.function.starts_with("<anonymous>") {
                "<anonymous>"
            } else {
                &report.function
            };
            let function_id = format!("{}::{}", normalized_file, function_symbol);
            let callees_json =
                serde_json::to_string(&report.callees).unwrap_or_else(|_| "[]".to_string());
            stmt.execute(params![
                sha,
                function_id,
                report.file,
                report.line as i64,
                report.language.name(),
                report.metrics.cc as i64,
                report.metrics.nd as i64,
                report.metrics.fo as i64,
                report.metrics.ns as i64,
                report.metrics.loc as i64,
                report.lrs,
                report.band.as_str(),
                report.suppression_reason,
                callees_json,
            ])
            .context("failed to insert report row")?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Load lean rows for call graph construction: (function_id, file, callees_json).
    ///
    /// Reads only the three columns needed to build a CallGraph, avoiding the ~23 MB
    /// cost of deserializing all enrichment columns.
    pub fn load_callee_rows(&self, sha: &str) -> Result<Vec<(String, String, Vec<String>)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT function_id, file, callees FROM functions WHERE commit_sha = ?1")?;
        let rows = stmt
            .query_map([sha], |row| {
                let function_id: String = row.get(0)?;
                let file: String = row.get(1)?;
                let callees_json: Option<String> = row.get(2)?;
                Ok((function_id, file, callees_json))
            })?
            .map(|r| {
                r.map(|(fid, file, cj)| {
                    let callees: Vec<String> = cj
                        .as_deref()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or_default();
                    (fid, file, callees)
                })
                .context("failed to read callee row")
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Update churn columns from a file-level churn map.
    ///
    /// Churn is stored per function row, keyed by file.
    pub fn update_churn(
        &self,
        sha: &str,
        file_churns: &std::collections::HashMap<String, crate::git::FileChurn>,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let mut stmt = self.conn.prepare(
            "UPDATE functions
             SET churn_added = ?1, churn_deleted = ?2
             WHERE commit_sha = ?3 AND file = ?4",
        )?;
        for (path, churn) in file_churns {
            stmt.execute(params![
                churn.lines_added as i64,
                churn.lines_deleted as i64,
                sha,
                path,
            ])
            .context("failed to update churn row")?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Compute fan-in from the call graph and batch-UPDATE it into the DB, then the
    /// caller should drop the CallGraph to free its memory.
    pub fn update_callgraph_metrics(
        &self,
        sha: &str,
        graph: &crate::callgraph::CallGraph,
    ) -> Result<()> {
        let fan_in_map = graph.build_fan_in_map();

        let tx = self.conn.unchecked_transaction()?;
        let mut stmt = self.conn.prepare(
            "UPDATE functions SET fan_in = ?1 WHERE commit_sha = ?2 AND function_id = ?3",
        )?;

        // Iterate over all graph nodes (not just rows) so we only UPDATE functions
        // that are actually in the graph; others keep a NULL fan_in.
        for function_id in graph.all_ids() {
            stmt.execute(params![
                fan_in_map.get(function_id).copied().unwrap_or(0) as i64,
                sha,
                function_id,
            ])
            .context("failed to update callgraph metrics row")?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Return the number of function rows for the given commit SHA.
    pub fn function_count(&self, sha: &str) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM functions WHERE commit_sha = ?1",
                [sha],
                |r| r.get(0),
            )
            .context("failed to count functions")?;
        Ok(count as usize)
    }

    /// Load enriched function rows from the pipeline buffer as a Vec<FunctionSnapshot>.
    ///
    /// Call this after `update_churn` and `update_callgraph_metrics` — the returned
    /// Vec will have `churn` and `callgraph` already populated. Touch metrics, activity
    /// risk and tiers are populated by the caller via
    /// the existing `SnapshotEnricher`.
    pub fn load_snapshot_functions(&self, sha: &str) -> Result<Vec<FunctionSnapshot>> {
        load_functions(&self.conn, sha)
    }

    /// Insert all functions from a snapshot into the temp database.
    pub fn insert_snapshot(&self, snapshot: &Snapshot) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        insert_commit(&self.conn, &snapshot.commit)?;
        insert_functions(&self.conn, snapshot)?;
        tx.commit()?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// SnapshotDb — persistent database at `.hotspots/snapshots.db`
// ---------------------------------------------------------------------------

/// Persistent SQLite snapshot database stored at `.hotspots/snapshots.db`.
///
/// Alternative to per-commit `.json.zst` files: stores all commits in a
/// single queryable database. Backward-compatible with existing `.json.zst`
/// files (those are still loaded when this DB doesn't have the requested SHA).
pub struct SnapshotDb {
    conn: Connection,
}

impl SnapshotDb {
    /// Open (or create) the snapshot database at the given path.
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory: {}", parent.display()))?;
        }
        let conn = Connection::open(db_path)
            .with_context(|| format!("failed to open {}", db_path.display()))?;
        // WAL mode: better concurrent read performance; not critical here but good practice.
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        apply_schema(&conn)?;
        Ok(SnapshotDb { conn })
    }

    /// Persist a snapshot, replacing any existing row for the same SHA.
    pub fn insert(&self, snapshot: &Snapshot) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        insert_commit(&self.conn, &snapshot.commit)?;
        // Delete existing function rows for this SHA before re-inserting (force-overwrite).
        self.conn.execute(
            "DELETE FROM functions WHERE commit_sha = ?1",
            [&snapshot.commit.sha],
        )?;
        insert_functions(&self.conn, snapshot)?;
        tx.commit()?;
        Ok(())
    }

    /// Load a snapshot for the given commit SHA, returning None if not found.
    pub fn load(&self, sha: &str) -> Result<Option<Snapshot>> {
        use crate::snapshot::{AnalysisInfo, FORMULA_VERSION, SNAPSHOT_SCHEMA_VERSION};

        // Type alias avoids clippy::type_complexity for the query row.
        type CommitRow = (
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<i64>,
            Option<String>,
            Option<String>,
        );

        // Check commit exists
        let commit_row: Option<CommitRow> = self
            .conn
            .query_row(
                "SELECT timestamp, branch, message, author,
                        is_fix_commit, is_revert_commit, ticket_ids, parents
                 FROM commits WHERE sha = ?1",
                [sha],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                    ))
                },
            )
            .optional()
            .context("failed to query commit")?;

        let Some((
            timestamp,
            branch,
            message,
            author,
            is_fix,
            is_revert,
            ticket_ids_json,
            parents_json,
        )) = commit_row
        else {
            return Ok(None);
        };

        let ticket_ids: Vec<String> = ticket_ids_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        let parents: Vec<String> = parents_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        let commit = CommitInfo {
            sha: sha.to_string(),
            parents,
            timestamp,
            branch,
            message,
            author,
            is_fix_commit: is_fix.map(|n| n != 0),
            is_revert_commit: is_revert.map(|n| n != 0),
            ticket_ids,
        };

        let functions = load_functions(&self.conn, sha)?;

        Ok(Some(Snapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            commit,
            analysis: AnalysisInfo {
                scope: "full".to_string(),
                tool_version: env!("CARGO_PKG_VERSION").to_string(),
                formula_version: FORMULA_VERSION,
                touch_window_days: 365,
            },
            functions,
            summary: None,
            aggregates: None,
            triage: None,
            architecture: None,
            co_change: None,
        }))
    }

    /// Check whether a snapshot for the given SHA exists in the database.
    pub fn contains(&self, sha: &str) -> Result<bool> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM commits WHERE sha = ?1", [sha], |r| {
                r.get(0)
            })
            .context("failed to query commit count")?;
        Ok(count > 0)
    }

    /// Return all commit SHAs stored in the database, ordered by timestamp ascending.
    pub fn all_shas(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT sha FROM commits ORDER BY timestamp ASC, sha ASC")?;
        let shas = stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()
            .context("failed to list SHAs")?;
        Ok(shas)
    }
}

/// Returns the path to the persistent snapshot database.
pub fn db_path(repo_root: &Path) -> std::path::PathBuf {
    crate::snapshot::hotspots_dir(repo_root).join("snapshots.db")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::GitContext;
    use crate::report::{FunctionRiskReport, MetricsReport as ReportMetrics, RiskReport};
    use crate::snapshot::Snapshot;

    fn make_snapshot() -> Snapshot {
        let ctx = GitContext {
            head_sha: "deadbeef".to_string(),
            parent_shas: vec!["cafebabe".to_string()],
            timestamp: 1_700_000_000,
            branch: Some("main".to_string()),
            is_detached: false,
            message: Some("test commit".to_string()),
            author: Some("test".to_string()),
            is_fix_commit: Some(false),
            is_revert_commit: Some(false),
            ticket_ids: vec![],
        };
        let reports = vec![FunctionRiskReport {
            file: "src/foo.ts".to_string(),
            function: "handler".to_string(),
            line: 10,
            language: crate::language::Language::TypeScript,
            metrics: ReportMetrics {
                cc: 3,
                nd: 1,
                fo: 2,
                ns: 0,
                loc: 20,
                missing_docstring: false,
            },
            risk: RiskReport {
                r_cc: 1.0,
                r_nd: 0.5,
                r_fo: 0.5,
                r_ns: 0.0,
            },
            lrs: 2.0,
            band: crate::risk::RiskBand::Low,
            callees: vec![],
            suppression_reason: None,
            explanation: None,
        }];
        Snapshot::new(ctx, reports)
    }

    #[test]
    fn test_snapshot_db_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let db_file = dir.path().join("snapshots.db");
        let snapshot = make_snapshot();

        let db = SnapshotDb::open(&db_file).unwrap();
        db.insert(&snapshot).unwrap();
        assert!(db.contains("deadbeef").unwrap());

        let loaded = db.load("deadbeef").unwrap().expect("should exist");
        assert_eq!(loaded.commit.sha, "deadbeef");
        assert_eq!(loaded.functions.len(), 1);
        assert_eq!(loaded.functions[0].function_id, "src/foo.ts::handler");
        assert_eq!(loaded.functions[0].metrics.cc, 3);

        assert!(db.load("nonexistent").unwrap().is_none());
    }

    #[test]
    fn test_snapshot_db_all_shas() {
        let dir = tempfile::tempdir().unwrap();
        let db_file = dir.path().join("snapshots.db");
        let snapshot = make_snapshot();

        let db = SnapshotDb::open(&db_file).unwrap();
        db.insert(&snapshot).unwrap();

        let shas = db.all_shas().unwrap();
        assert_eq!(shas, vec!["deadbeef"]);
    }

    #[test]
    fn test_snapshot_db_contains_missing() {
        let dir = tempfile::tempdir().unwrap();
        let db = SnapshotDb::open(&dir.path().join("s.db")).unwrap();
        assert!(!db.contains("nothere").unwrap());
    }

    #[test]
    fn test_snapshot_db_idempotent_reinsert() {
        // Inserting the same SHA twice must not duplicate function rows.
        let dir = tempfile::tempdir().unwrap();
        let db = SnapshotDb::open(&dir.path().join("s.db")).unwrap();
        let snapshot = make_snapshot();

        db.insert(&snapshot).unwrap();
        db.insert(&snapshot).unwrap();

        let loaded = db.load("deadbeef").unwrap().expect("should exist");
        assert_eq!(loaded.functions.len(), 1, "duplicate rows on re-insert");
    }

    #[test]
    fn test_snapshot_db_multi_commit_ordering() {
        // Two commits at different timestamps — all_shas must return oldest first.
        let dir = tempfile::tempdir().unwrap();
        let db = SnapshotDb::open(&dir.path().join("s.db")).unwrap();

        let snap1 = make_snapshot(); // timestamp 1_700_000_000
        let mut snap2 = make_snapshot();
        snap2.commit.sha = "aabbccdd".to_string();
        snap2.commit.timestamp = 1_700_000_001;
        snap2.functions[0].function_id = "src/bar.ts::helper".to_string();

        db.insert(&snap2).unwrap(); // insert newer first
        db.insert(&snap1).unwrap();

        let shas = db.all_shas().unwrap();
        assert_eq!(shas[0], "deadbeef", "older commit should be first");
        assert_eq!(shas[1], "aabbccdd");
    }

    #[test]
    fn test_snapshot_db_full_field_roundtrip() {
        use crate::report::{FunctionRiskReport, MetricsReport as ReportMetrics, RiskReport};
        use crate::scoring::RiskFactors;

        let dir = tempfile::tempdir().unwrap();
        let db = SnapshotDb::open(&dir.path().join("s.db")).unwrap();

        let ctx = crate::git::GitContext {
            head_sha: "fullsha".to_string(),
            parent_shas: vec!["p1".to_string()],
            timestamp: 1_700_000_000,
            branch: Some("feat/test".to_string()),
            is_detached: false,
            message: Some("full field test".to_string()),
            author: Some("author".to_string()),
            is_fix_commit: Some(true),
            is_revert_commit: Some(false),
            ticket_ids: vec!["PROJ-123".to_string()],
        };
        let report = FunctionRiskReport {
            file: "src/svc.ts".to_string(),
            function: "processRequest".to_string(),
            line: 42,
            language: crate::language::Language::TypeScript,
            metrics: ReportMetrics {
                cc: 8,
                nd: 3,
                fo: 5,
                ns: 2,
                loc: 100,
                missing_docstring: false,
            },
            risk: RiskReport {
                r_cc: 2.0,
                r_nd: 1.0,
                r_fo: 0.8,
                r_ns: 0.4,
            },
            lrs: 8.0,
            band: crate::risk::RiskBand::High,
            callees: vec!["doA".to_string(), "doB".to_string()],
            suppression_reason: None,
            explanation: None,
        };
        let mut snapshot = Snapshot::new(ctx, vec![report]);

        // Populate optional fields directly on the FunctionSnapshot.
        let f = &mut snapshot.functions[0];
        f.churn = Some(ChurnMetrics {
            lines_added: 50,
            lines_deleted: 10,
        });
        f.touch_count = Some(7);
        f.days_since_last_change = Some(5);
        f.callgraph = Some(CallGraphMetrics { fan_in: 3 });
        f.activity_risk = Some(9.5);
        f.risk_factors = Some(RiskFactors {
            complexity: 1.0,
            churn: 0.5,
            touch: 0.8,
            recency: 0.3,
        });
        f.tier = Some("active".to_string());

        db.insert(&snapshot).unwrap();
        let loaded = db.load("fullsha").unwrap().expect("should exist");
        let lf = &loaded.functions[0];

        let churn = lf.churn.as_ref().expect("churn should be present");
        assert_eq!(churn.lines_added, 50);
        assert_eq!(churn.lines_deleted, 10);
        assert_eq!(lf.touch_count, Some(7));
        assert_eq!(lf.days_since_last_change, Some(5));

        let cg = lf.callgraph.as_ref().expect("callgraph should be present");
        assert_eq!(cg.fan_in, 3);

        assert!((lf.activity_risk.unwrap() - 9.5).abs() < 1e-9);

        let rf = lf
            .risk_factors
            .as_ref()
            .expect("risk_factors should be present");
        assert!((rf.complexity - 1.0).abs() < 1e-9);

        assert_eq!(lf.tier.as_deref(), Some("active"));

        // Commit fields
        assert_eq!(loaded.commit.ticket_ids, vec!["PROJ-123"]);
        assert_eq!(loaded.commit.is_fix_commit, Some(true));
    }
}
