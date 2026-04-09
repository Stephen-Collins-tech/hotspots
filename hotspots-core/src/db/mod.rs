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

use crate::snapshot::{
    CallGraphMetrics, ChurnMetrics, CommitInfo, FunctionSnapshot, PercentileFlags, Snapshot,
};

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
    churn_added             INTEGER,
    churn_deleted           INTEGER,
    touch_count_30d         INTEGER,
    days_since_last_change  INTEGER,
    fan_in                  INTEGER,
    fan_out                 INTEGER,
    pagerank                REAL,
    betweenness             REAL,
    scc_id                  INTEGER,
    scc_size                INTEGER,
    is_entrypoint           INTEGER,
    dependency_depth        INTEGER,
    neighbor_churn          INTEGER,
    activity_risk           REAL,
    risk_factors            TEXT,
    is_top_10_pct           INTEGER,
    is_top_5_pct            INTEGER,
    is_top_1_pct            INTEGER,
    driver                  TEXT,
    driver_detail           TEXT,
    quadrant                TEXT,
    patterns                TEXT,
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
        "INSERT OR IGNORE INTO commits
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
            touch_count_30d, days_since_last_change,
            fan_in, fan_out, pagerank, betweenness,
            scc_id, scc_size, is_entrypoint, dependency_depth, neighbor_churn,
            activity_risk, risk_factors,
            is_top_10_pct, is_top_5_pct, is_top_1_pct,
            driver, driver_detail, quadrant, patterns
        ) VALUES (
            ?1,?2,?3,?4,?5,
            ?6,?7,?8,?9,?10,?11,?12,?13,
            ?14,?15,
            ?16,?17,
            ?18,?19,?20,?21,
            ?22,?23,?24,?25,?26,
            ?27,?28,
            ?29,?30,?31,
            ?32,?33,?34,?35
        )",
    )?;

    for func in &snapshot.functions {
        let risk_factors_json = func
            .risk_factors
            .as_ref()
            .and_then(|rf| serde_json::to_string(rf).ok());
        let patterns_json = serde_json::to_string(&func.patterns).unwrap_or_default();

        let (churn_added, churn_deleted) = func
            .churn
            .as_ref()
            .map(|c| (Some(c.lines_added as i64), Some(c.lines_deleted as i64)))
            .unwrap_or((None, None));

        let (
            fan_in,
            fan_out,
            pagerank,
            betweenness,
            scc_id,
            scc_size,
            is_entrypoint,
            dep_depth,
            nbr_churn,
        ) = func
            .callgraph
            .as_ref()
            .map(|cg| {
                (
                    Some(cg.fan_in as i64),
                    Some(cg.fan_out as i64),
                    Some(cg.pagerank),
                    Some(cg.betweenness),
                    Some(cg.scc_id as i64),
                    Some(cg.scc_size as i64),
                    Some(cg.is_entrypoint as i64),
                    cg.dependency_depth.map(|d| d as i64),
                    cg.neighbor_churn.map(|n| n as i64),
                )
            })
            .unwrap_or((None, None, None, None, None, None, None, None, None));

        let (top10, top5, top1) = func
            .percentile
            .as_ref()
            .map(|p| {
                (
                    Some(p.is_top_10_pct as i64),
                    Some(p.is_top_5_pct as i64),
                    Some(p.is_top_1_pct as i64),
                )
            })
            .unwrap_or((None, None, None));

        stmt.execute(params![
            sha,
            func.function_id,
            func.file,
            func.line as i64,
            func.language,
            func.metrics.cc as i64,
            func.metrics.nd as i64,
            func.metrics.fo as i64,
            func.metrics.ns as i64,
            func.metrics.loc as i64,
            func.lrs,
            func.band,
            func.suppression_reason,
            churn_added,
            churn_deleted,
            func.touch_count_30d.map(|n| n as i64),
            func.days_since_last_change.map(|n| n as i64),
            fan_in,
            fan_out,
            pagerank,
            betweenness,
            scc_id,
            scc_size,
            is_entrypoint,
            dep_depth,
            nbr_churn,
            func.activity_risk,
            risk_factors_json,
            top10,
            top5,
            top1,
            func.driver,
            func.driver_detail,
            func.quadrant,
            patterns_json,
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
                touch_count_30d, days_since_last_change,
                fan_in, fan_out, pagerank, betweenness,
                scc_id, scc_size, is_entrypoint, dependency_depth, neighbor_churn,
                activity_risk, risk_factors,
                is_top_10_pct, is_top_5_pct, is_top_1_pct,
                driver, driver_detail, quadrant, patterns
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
        let churn = churn_added.zip(churn_deleted).map(|(a, d)| {
            let net = a - d;
            ChurnMetrics {
                lines_added: a as usize,
                lines_deleted: d as usize,
                net_change: net,
            }
        });

        let touch_count_30d: Option<i64> = row.get(14)?;
        let days_since_last_change: Option<i64> = row.get(15)?;

        let fan_in: Option<i64> = row.get(16)?;
        let fan_out: Option<i64> = row.get(17)?;
        let pagerank: Option<f64> = row.get(18)?;
        let betweenness: Option<f64> = row.get(19)?;
        let scc_id: Option<i64> = row.get(20)?;
        let scc_size: Option<i64> = row.get(21)?;
        let is_entrypoint: Option<i64> = row.get(22)?;
        let dep_depth: Option<i64> = row.get(23)?;
        let nbr_churn: Option<i64> = row.get(24)?;
        let callgraph = fan_in
            .zip(fan_out)
            .zip(pagerank)
            .zip(betweenness)
            .zip(scc_id)
            .zip(scc_size)
            .zip(is_entrypoint)
            .map(|((((((fi, fo), pr), bt), si), ss), ep)| CallGraphMetrics {
                fan_in: fi as usize,
                fan_out: fo as usize,
                pagerank: pr,
                betweenness: bt,
                scc_id: si as usize,
                scc_size: ss as usize,
                is_entrypoint: ep != 0,
                dependency_depth: dep_depth.map(|d| d as usize),
                neighbor_churn: nbr_churn.map(|n| n as usize),
            });

        let activity_risk: Option<f64> = row.get(25)?;
        let risk_factors_json: Option<String> = row.get(26)?;

        let top10: Option<i64> = row.get(27)?;
        let top5: Option<i64> = row.get(28)?;
        let top1: Option<i64> = row.get(29)?;
        let percentile = top10
            .zip(top5)
            .zip(top1)
            .map(|((t10, t5), t1)| PercentileFlags {
                is_top_10_pct: t10 != 0,
                is_top_5_pct: t5 != 0,
                is_top_1_pct: t1 != 0,
            });

        let driver: Option<String> = row.get(30)?;
        let driver_detail: Option<String> = row.get(31)?;
        let quadrant: Option<String> = row.get(32)?;
        let patterns_json: Option<String> = row.get(33)?;

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
            touch_count_30d,
            days_since_last_change,
            callgraph,
            activity_risk,
            risk_factors_json,
            percentile,
            driver,
            driver_detail,
            quadrant,
            patterns_json,
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
            touch_count_30d,
            days_since_last_change,
            callgraph,
            activity_risk,
            risk_factors_json,
            percentile,
            driver,
            driver_detail,
            quadrant,
            patterns_json,
        ) = row.context("failed to read function row")?;

        let risk_factors = risk_factors_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());

        let patterns: Vec<String> = patterns_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        functions.push(FunctionSnapshot {
            function_id,
            file,
            line: line as u32,
            language,
            metrics: MetricsReport {
                cc: cc as usize,
                nd: nd as usize,
                fo: fo as usize,
                ns: ns as usize,
                loc: loc as usize,
            },
            lrs,
            band,
            suppression_reason,
            churn,
            touch_count_30d: touch_count_30d.map(|n| n as usize),
            days_since_last_change: days_since_last_change.map(|n| n as u32),
            callgraph,
            activity_risk,
            risk_factors,
            percentile,
            driver,
            driver_detail,
            quadrant,
            patterns,
            pattern_details: None,
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

    /// Insert all functions from a snapshot into the temp database.
    pub fn insert_snapshot(&self, snapshot: &Snapshot) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        insert_commit(&self.conn, &snapshot.commit)?;
        insert_functions(&self.conn, snapshot)?;
        tx.commit()?;
        Ok(())
    }

    /// Compute activity-risk percentile thresholds via SQL window functions.
    ///
    /// Returns `(p90, p95, p99)` activity_risk values (falling back to lrs when
    /// activity_risk is NULL) for the given commit. Used by T4.5 aggregation.
    pub fn percentile_thresholds(&self, sha: &str) -> Result<(f64, f64, f64)> {
        // NTILE(100) assigns each row a bucket 1..100; we want the minimum score
        // in buckets 90, 95, 99 (i.e., the value at that percentile).
        let mut stmt = self.conn.prepare(
            "WITH ranked AS (
                SELECT COALESCE(activity_risk, lrs) AS score,
                       NTILE(100) OVER (ORDER BY COALESCE(activity_risk, lrs)) AS bucket
                FROM functions
                WHERE commit_sha = ?1
             )
             SELECT
                MIN(CASE WHEN bucket >= 90 THEN score END),
                MIN(CASE WHEN bucket >= 95 THEN score END),
                MIN(CASE WHEN bucket >= 99 THEN score END)
             FROM ranked",
        )?;

        let (p90, p95, p99) = stmt
            .query_row([sha], |row| {
                let p90: Option<f64> = row.get(0)?;
                let p95: Option<f64> = row.get(1)?;
                let p99: Option<f64> = row.get(2)?;
                Ok((p90.unwrap_or(0.0), p95.unwrap_or(0.0), p99.unwrap_or(0.0)))
            })
            .context("failed to compute percentile thresholds")?;

        Ok((p90, p95, p99))
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
        use crate::snapshot::{AnalysisInfo, SNAPSHOT_SCHEMA_VERSION};

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
            },
            functions,
            summary: None,
            aggregates: None,
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
            language: "TypeScript".to_string(),
            metrics: ReportMetrics {
                cc: 3,
                nd: 1,
                fo: 2,
                ns: 0,
                loc: 20,
            },
            risk: RiskReport {
                r_cc: 1.0,
                r_nd: 0.5,
                r_fo: 0.5,
                r_ns: 0.0,
            },
            lrs: 2.0,
            band: "low".to_string(),
            callees: vec![],
            suppression_reason: None,
            patterns: vec![],
            pattern_details: None,
        }];
        Snapshot::new(ctx, reports)
    }

    #[test]
    fn test_temp_db_insert_and_percentile() {
        let snapshot = make_snapshot();
        let db = TempDb::new().unwrap();
        db.insert_snapshot(&snapshot).unwrap();
        let (p90, p95, p99) = db.percentile_thresholds("deadbeef").unwrap();
        // Single-function snapshot: all percentile thresholds are the function's own lrs.
        assert!(p90 >= 0.0);
        assert!(p95 >= p90);
        assert!(p99 >= p95);
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
}
