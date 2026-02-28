//! HTML report generation
//!
//! Generates self-contained HTML reports with embedded CSS and JavaScript.
//! Reports are interactive (sorting, filtering) and work offline.

use crate::aggregates::SnapshotAggregates;
use crate::delta::{Delta, FunctionDeltaEntry, FunctionStatus};
use crate::policy::{PolicyId, PolicyResults};
use crate::snapshot::{CommitInfo, FunctionSnapshot, Snapshot, SnapshotSummary};

/// Render a snapshot as an HTML report
pub fn render_html_snapshot(
    snapshot: &Snapshot,
    history: &[(CommitInfo, SnapshotSummary)],
) -> String {
    let aggregates = snapshot.aggregates.as_ref();
    let history_json = render_history_json(history);
    let trends = if history_json == "[]" {
        String::new()
    } else {
        render_trends_section(&history_json)
    };
    let patterns_breakdown = render_pattern_breakdown(&snapshot.functions);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Hotspots Report - {sha}</title>
    <style>{css}</style>
</head>
<body>
    <div class="container">
        {header}
        {summary}
        {trends}
        {triage}
        {patterns_breakdown}
        {functions_table}
        {aggregates_section}
        {footer}
    </div>
    <script>{js}</script>
</body>
</html>"#,
        sha = &snapshot.commit.sha[..8],
        css = inline_css(),
        js = inline_javascript(),
        header = render_header(&snapshot.commit),
        summary = render_summary(snapshot),
        trends = trends,
        triage = render_triage_panel(&snapshot.functions),
        patterns_breakdown = patterns_breakdown,
        functions_table = render_functions_table(&snapshot.functions),
        aggregates_section = aggregates.map(render_aggregates).unwrap_or_default(),
        footer = render_footer(),
    )
}

/// Serialize history into a JSON array for embedding in the HTML report.
/// Returns `"[]"` when there are fewer than 2 data points.
fn render_history_json(history: &[(CommitInfo, SnapshotSummary)]) -> String {
    if history.len() < 2 {
        return "[]".to_string();
    }
    let entries: Vec<String> = history
        .iter()
        .map(|(commit, summary)| {
            let critical = summary.by_band.get("critical").map(|b| b.count).unwrap_or(0);
            let high = summary.by_band.get("high").map(|b| b.count).unwrap_or(0);
            let moderate = summary.by_band.get("moderate").map(|b| b.count).unwrap_or(0);
            let low = summary.by_band.get("low").map(|b| b.count).unwrap_or(0);
            format!(
                r#"{{"ts":{},"sha":"{}","critical":{},"high":{},"moderate":{},"low":{},"risk":{:.2},"share":{:.4}}}"#,
                commit.timestamp,
                &commit.sha[..commit.sha.len().min(8)],
                critical,
                high,
                moderate,
                low,
                summary.total_activity_risk,
                summary.top_1_pct_share,
            )
        })
        .collect();
    format!("[{}]", entries.join(","))
}

/// Render the Trends section HTML (including the embedded history JSON script tag).
fn render_trends_section(json: &str) -> String {
    format!(
        r#"<script>window.__hsHistory = {json};</script>
<section class="section trends-section" id="trends">
    <h2>Trends</h2>
    <div class="chart-label">Risk Band Distribution Over Time</div>
    <canvas id="hs-bands-chart" height="220"></canvas>
    <div class="trends-charts">
        <div>
            <div class="chart-label">Total Activity Risk</div>
            <canvas id="hs-risk-chart" height="180"></canvas>
        </div>
        <div>
            <div class="chart-label">Top-1% Risk Concentration</div>
            <canvas id="hs-share-chart" height="180"></canvas>
        </div>
    </div>
</section>"#,
        json = json,
    )
}

/// Render a delta as an HTML report
pub fn render_html_delta(delta: &Delta) -> String {
    let commit_sha = &delta.commit.sha[..8];

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Hotspots Delta Report - {sha}</title>
    <style>{css}</style>
</head>
<body>
    <div class="container">
        {header}
        {summary}
        {policy_section}
        {delta_table}
        {footer}
    </div>
    <script>{js}</script>
</body>
</html>"#,
        sha = commit_sha,
        css = inline_css(),
        js = inline_javascript(),
        header = render_delta_header(&delta.commit),
        summary = render_delta_summary(delta),
        policy_section = delta
            .policy
            .as_ref()
            .map(|p| render_policy_section(p, delta))
            .unwrap_or_default(),
        delta_table = render_delta_table(&delta.deltas),
        footer = render_footer(),
    )
}

/// Inline CSS styles
fn inline_css() -> &'static str {
    r#"
/* Reset & Base */
* {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
}

body {
    font-family: system-ui, -apple-system, 'Segoe UI', sans-serif;
    line-height: 1.6;
    color: #111827;
    background: #ffffff;
}

/* Container */
.container {
    max-width: 1400px;
    margin: 0 auto;
    padding: 2rem;
}

/* Header */
header {
    margin-bottom: 2rem;
    padding-bottom: 1rem;
    border-bottom: 2px solid #e5e7eb;
}

header h1 {
    font-size: 2rem;
    font-weight: 700;
    margin-bottom: 0.5rem;
}

header .meta {
    color: #6b7280;
    font-size: 0.875rem;
}

/* Summary */
.summary {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 1rem;
    margin-bottom: 2rem;
}

.summary-card {
    background: #f9fafb;
    padding: 1rem;
    border-radius: 0.5rem;
    border-left: 4px solid #3b82f6;
}

.summary-card h3 {
    font-size: 0.875rem;
    font-weight: 600;
    color: #6b7280;
    margin-bottom: 0.5rem;
}

.summary-card .value {
    font-size: 1.5rem;
    font-weight: 700;
}

/* Section */
.section {
    margin-bottom: 2rem;
}

.section h2 {
    font-size: 1.5rem;
    font-weight: 700;
    margin-bottom: 1rem;
}

/* Table */
table {
    width: 100%;
    border-collapse: collapse;
    background: #ffffff;
    border-radius: 0.5rem;
    overflow: hidden;
}

thead {
    background: #f9fafb;
}

th {
    padding: 0.75rem;
    text-align: left;
    font-weight: 600;
    font-size: 0.875rem;
    color: #374151;
    border-bottom: 2px solid #e5e7eb;
}

td {
    padding: 0.75rem;
    border-bottom: 1px solid #e5e7eb;
    font-size: 0.875rem;
}

tr:last-child td {
    border-bottom: none;
}

tbody tr:hover {
    background: #f3f4f6;
}

/* Risk Bands */
.band-low {
    color: #22c55e;
    font-weight: 600;
}

.band-moderate {
    color: #eab308;
    font-weight: 600;
}

.band-high {
    color: #f97316;
    font-weight: 600;
}

.band-critical {
    color: #ef4444;
    font-weight: 600;
}

/* Code/Monospace */
.monospace {
    font-family: 'Monaco', 'Courier New', monospace;
    font-size: 0.875rem;
}

/* Footer */
footer {
    margin-top: 3rem;
    padding-top: 1rem;
    border-top: 1px solid #e5e7eb;
    text-align: center;
    color: #6b7280;
    font-size: 0.875rem;
}

/* Mobile */
@media (max-width: 768px) {
    .container {
        padding: 1rem;
    }

    header h1 {
        font-size: 1.5rem;
    }

    .summary {
        grid-template-columns: 1fr;
    }

    table {
        font-size: 0.75rem;
    }

    th, td {
        padding: 0.5rem;
    }
}

/* Filters */
.filters {
    display: flex;
    gap: 1rem;
    margin-bottom: 1rem;
    flex-wrap: wrap;
}

.filter-group {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
}

.filter-group label {
    font-size: 0.75rem;
    font-weight: 600;
    color: #6b7280;
}

.filter-group select,
.filter-group input {
    padding: 0.5rem;
    border: 1px solid #d1d5db;
    border-radius: 0.375rem;
    font-size: 0.875rem;
}

.filter-group select:focus,
.filter-group input:focus {
    outline: none;
    border-color: #3b82f6;
}

/* Triage section */
.triage-section {
    border: 2px solid #f59e0b;
    border-radius: 0.5rem;
    padding: 1.5rem;
    background: #fffbeb;
    margin-bottom: 2rem;
}

.triage-section h2 {
    color: #92400e;
    margin-bottom: 1rem;
}

.triage-subtitle {
    font-size: 1rem;
    font-weight: 600;
    margin-bottom: 0.75rem;
    color: #374151;
}

/* Quadrant chips */
.quadrant-chips {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.75rem;
    margin-bottom: 1.5rem;
}

@media (max-width: 768px) {
    .quadrant-chips {
        grid-template-columns: repeat(2, 1fr);
    }
}

.quadrant-chip {
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    border-left: 4px solid;
    text-align: center;
}

.quadrant-fire   { border-left-color: #ef4444; background: #fef2f2; }
.quadrant-debt   { border-left-color: #8b5cf6; background: #f5f3ff; }
.quadrant-watch  { border-left-color: #f59e0b; background: #fffbeb; }
.quadrant-ok     { border-left-color: #22c55e; background: #f0fdf4; }

.chip-label {
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #6b7280;
}

.chip-count {
    font-size: 2rem;
    font-weight: 800;
    line-height: 1.2;
}

.chip-desc {
    font-size: 0.7rem;
    color: #9ca3af;
    margin-top: 0.15rem;
}

.quadrant-fire .chip-count  { color: #ef4444; }
.quadrant-debt .chip-count  { color: #8b5cf6; }
.quadrant-watch .chip-count { color: #f59e0b; }
.quadrant-ok .chip-count    { color: #22c55e; }

/* Active risk row highlight */
.triage-active-row { background: #fff7ed; }
.triage-active-row:hover { background: #fef3c7; }

/* Recency colors */
.recency-hot  { color: #ef4444; font-weight: 600; }
.recency-warm { color: #f97316; font-weight: 600; }
.recency-cool { color: #6b7280; }
.recency-cold { color: #d1d5db; }

/* Safe-to-refactor indicator */
.refactor-ready { color: #22c55e; font-weight: 600; }

/* Pattern pill badges */
.pattern {
    display: inline-block;
    font-size: 0.68rem;
    font-weight: 600;
    padding: 0.15rem 0.45rem;
    border-radius: 999px;
    border: 1px solid transparent;
    font-family: monospace;
    cursor: default;
    white-space: nowrap;
}
.pattern-cell { display: flex; flex-wrap: wrap; gap: 3px; align-items: center; }

/* Tier 1 — structural (warm palette) */
.pattern-complex_branching { background: #fffbeb; color: #b45309; border-color: #fde68a; }
.pattern-deeply_nested     { background: #fff7ed; color: #c2410c; border-color: #fed7aa; }
.pattern-exit_heavy        { background: #f5f3ff; color: #7c3aed; border-color: #ddd6fe; }
.pattern-god_function      { background: #fef2f2; color: #dc2626; border-color: #fecaca; }
.pattern-long_function     { background: #fff1f2; color: #be123c; border-color: #fecdd3; }
/* Tier 2 — behavioral (cool palette) */
.pattern-churn_magnet      { background: #eff6ff; color: #1d4ed8; border-color: #bfdbfe; }
.pattern-cyclic_hub        { background: #fdf4ff; color: #a21caf; border-color: #f0abfc; }
.pattern-hub_function      { background: #eef2ff; color: #4338ca; border-color: #c7d2fe; }
.pattern-middle_man        { background: #f1f5f9; color: #475569; border-color: #cbd5e1; }
.pattern-neighbor_risk     { background: #f0fdfa; color: #0f766e; border-color: #99f6e4; }
.pattern-shotgun_target    { background: #fdf2f8; color: #be185d; border-color: #fbcfe8; }
.pattern-stale_complex     { background: #fefce8; color: #854d0e; border-color: #fef08a; }
/* volatile_god — derived, most severe: inverted dark badge */
.pattern-volatile_god      { background: #7f1d1d; color: #fef2f2; border-color: #991b1b; }

/* Pattern breakdown widget */
.pattern-breakdown {
    border: 1px solid #e5e7eb;
    border-radius: 0.5rem;
    padding: 1.5rem;
    background: #f9fafb;
    margin-bottom: 2rem;
}
.pattern-breakdown h2 { color: #374151; margin-bottom: 0.4rem; font-size: 1.1rem; }
.pattern-breakdown-subtitle { font-size: 0.85rem; color: #6b7280; margin-bottom: 1rem; }
.pattern-chips { display: flex; flex-wrap: wrap; gap: 0.75rem; }
.pattern-chip {
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    border-left: 4px solid;
    min-width: 130px;
    flex: 1 1 130px;
    max-width: 185px;
}
.pattern-chip-count { font-size: 1.75rem; font-weight: 800; line-height: 1.2; }
.pattern-chip-name  { font-size: 0.7rem; font-weight: 700; font-family: monospace; margin-top: 0.25rem; }
.pattern-chip-desc  { font-size: 0.65rem; color: #9ca3af; margin-top: 0.15rem; }

.pattern-chip-complex_branching { border-left-color: #b45309; background: #fffbeb; }
.pattern-chip-complex_branching .pattern-chip-count { color: #b45309; }
.pattern-chip-deeply_nested     { border-left-color: #c2410c; background: #fff7ed; }
.pattern-chip-deeply_nested     .pattern-chip-count { color: #c2410c; }
.pattern-chip-exit_heavy        { border-left-color: #7c3aed; background: #f5f3ff; }
.pattern-chip-exit_heavy        .pattern-chip-count { color: #7c3aed; }
.pattern-chip-god_function      { border-left-color: #dc2626; background: #fef2f2; }
.pattern-chip-god_function      .pattern-chip-count { color: #dc2626; }
.pattern-chip-long_function     { border-left-color: #be123c; background: #fff1f2; }
.pattern-chip-long_function     .pattern-chip-count { color: #be123c; }
.pattern-chip-churn_magnet      { border-left-color: #1d4ed8; background: #eff6ff; }
.pattern-chip-churn_magnet      .pattern-chip-count { color: #1d4ed8; }
.pattern-chip-cyclic_hub        { border-left-color: #a21caf; background: #fdf4ff; }
.pattern-chip-cyclic_hub        .pattern-chip-count { color: #a21caf; }
.pattern-chip-hub_function      { border-left-color: #4338ca; background: #eef2ff; }
.pattern-chip-hub_function      .pattern-chip-count { color: #4338ca; }
.pattern-chip-middle_man        { border-left-color: #475569; background: #f1f5f9; }
.pattern-chip-middle_man        .pattern-chip-count { color: #475569; }
.pattern-chip-neighbor_risk     { border-left-color: #0f766e; background: #f0fdfa; }
.pattern-chip-neighbor_risk     .pattern-chip-count { color: #0f766e; }
.pattern-chip-shotgun_target    { border-left-color: #be185d; background: #fdf2f8; }
.pattern-chip-shotgun_target    .pattern-chip-count { color: #be185d; }
.pattern-chip-stale_complex     { border-left-color: #854d0e; background: #fefce8; }
.pattern-chip-stale_complex     .pattern-chip-count { color: #854d0e; }
.pattern-chip-volatile_god      { border-left-color: #7f1d1d; background: #fef2f2; }
.pattern-chip-volatile_god      .pattern-chip-count { color: #7f1d1d; }

/* Driver badges */
.driver-badge { font-size: 0.75rem; padding: 0.15rem 0.4rem; border-radius: 0.25rem; margin-left: 0.4rem; }
.driver-high_complexity    { background: #fff3e0; color: #e65100; }
.driver-deep_nesting       { background: #f3e5f5; color: #6a1b9a; }
.driver-high_churn_low_cc  { background: #e0f7fa; color: #006064; }
.driver-high_fanin_complex { background: #e3f2fd; color: #0d47a1; }
.driver-high_fanout_churning { background: #e8f5e9; color: #1b5e20; }
.driver-cyclic_dep         { background: #fce4ec; color: #880e4f; }
.driver-composite          { background: #f5f5f5; color: #424242; }

/* Module instability zones (Robert Martin) */
.zone-pain             { color: #ef4444; font-weight: 600; }
.zone-stable           { color: #22c55e; }
.zone-balanced         { color: #3b82f6; }
.zone-volatile         { color: #f97316; font-weight: 600; }
.zone-volatile-complex { color: #ef4444; font-weight: 600; }

/* Module and co-change risk */
.module-risk-high   { color: #ef4444; font-weight: 600; }
.co-change-high     { color: #ef4444; font-weight: 600; }
.co-change-moderate { color: #f97316; font-weight: 600; }

/* Pagination */
.pagination-controls {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-bottom: 0.75rem;
    flex-wrap: wrap;
}

.page-buttons {
    display: flex;
    align-items: center;
    gap: 0.25rem;
}

.page-btn, .page-nav {
    padding: 0.25rem 0.6rem;
    border: 1px solid #d1d5db;
    border-radius: 0.375rem;
    background: #ffffff;
    cursor: pointer;
    font-size: 0.8125rem;
    line-height: 1.5;
}

.page-btn:hover:not(:disabled), .page-nav:hover:not(:disabled) {
    background: #f3f4f6;
    border-color: #9ca3af;
}

.page-btn.active, .page-btn:disabled {
    background: #3b82f6;
    color: #ffffff;
    border-color: #3b82f6;
    cursor: default;
}

.page-nav:disabled {
    opacity: 0.4;
    cursor: not-allowed;
}

.page-info {
    font-size: 0.8125rem;
    color: #6b7280;
    white-space: nowrap;
}

.page-ellipsis {
    padding: 0 0.25rem;
    color: #9ca3af;
    font-size: 0.8125rem;
}

.page-size-select {
    padding: 0.25rem 0.5rem;
    border: 1px solid #d1d5db;
    border-radius: 0.375rem;
    font-size: 0.8125rem;
    background: #ffffff;
}

/* Sortable headers */
th.sortable {
    cursor: pointer;
    user-select: none;
}

th.sortable:hover {
    background: #e5e7eb;
}

th.sortable::after {
    content: ' ↕';
    opacity: 0.3;
}

th.sortable.asc::after {
    content: ' ↑';
    opacity: 1;
}

th.sortable.desc::after {
    content: ' ↓';
    opacity: 1;
}

/* Triage action column */
.triage-action { font-size: 0.8rem; color: #4b5563; font-style: italic; }

/* Trend charts */
.trends-section canvas { display:block; border-radius:0.375rem; background:#f9fafb; width:100%; }
.trends-charts { display:grid; grid-template-columns:1fr 1fr; gap:1rem; margin-top:1rem; }
@media (max-width:768px) { .trends-charts { grid-template-columns:1fr; } }
.chart-label { font-size:0.75rem; font-weight:600; color:#6b7280; margin-bottom:0.25rem; }

/* Dark Mode */
@media (prefers-color-scheme: dark) {
    body {
        background: #111827;
        color: #f9fafb;
    }

    header {
        border-bottom-color: #374151;
    }

    .summary-card {
        background: #1f2937;
    }

    .filter-group label {
        color: #9ca3af;
    }

    .filter-group select,
    .filter-group input {
        background: #1f2937;
        border-color: #374151;
        color: #f9fafb;
    }

    thead {
        background: #1f2937;
    }

    th {
        color: #f9fafb;
        border-bottom-color: #374151;
    }

    th.sortable:hover {
        background: #374151;
    }

    td {
        border-bottom-color: #374151;
    }

    tbody tr:hover {
        background: #1f2937;
    }

    table {
        background: #111827;
    }

    footer {
        border-top-color: #374151;
    }

    .driver-high_complexity    { background: #3d2000; color: #ffab76; }
    .driver-deep_nesting       { background: #2d0050; color: #e0b0ff; }
    .driver-high_churn_low_cc  { background: #002022; color: #80deea; }
    .driver-high_fanin_complex { background: #001e3c; color: #90caf9; }
    .driver-high_fanout_churning { background: #002200; color: #a5d6a7; }
    .driver-cyclic_dep         { background: #3b0016; color: #f48fb1; }
    .driver-composite          { background: #1a1a1a; color: #bdbdbd; }

    .zone-stable   { color: #4ade80; }
    .zone-balanced { color: #60a5fa; }

    .page-btn, .page-nav { background: #1f2937; border-color: #374151; color: #f9fafb; }
    .page-btn:hover:not(:disabled), .page-nav:hover:not(:disabled) { background: #374151; }
    .page-size-select { background: #1f2937; border-color: #374151; color: #f9fafb; }

    .triage-section { border-color: #92400e; background: #1c1500; }
    .triage-section h2 { color: #fbbf24; }
    .triage-subtitle { color: #d1d5db; }
    .quadrant-fire   { background: #1a0000; }
    .quadrant-debt   { background: #140028; }
    .quadrant-watch  { background: #1a1000; }
    .quadrant-ok     { background: #001a08; }
    .chip-label { color: #9ca3af; }
    .chip-desc  { color: #6b7280; }
    .triage-active-row { background: #1c1200; }
    .triage-active-row:hover { background: #2a1a00; }
    .recency-cold { color: #4b5563; }
    .triage-action { color: #9ca3af; }
    .trends-section canvas { background:#1f2937; }

    /* Pattern badges — dark mode */
    .pattern-complex_branching { background: #2d1b00; color: #fbbf24; border-color: #92400e; }
    .pattern-deeply_nested     { background: #3a1500; color: #fb923c; border-color: #c2410c; }
    .pattern-exit_heavy        { background: #1e0050; color: #c4b5fd; border-color: #6d28d9; }
    .pattern-god_function      { background: #3a0000; color: #fca5a5; border-color: #991b1b; }
    .pattern-long_function     { background: #3b0018; color: #fda4af; border-color: #9f1239; }
    .pattern-churn_magnet      { background: #001a3d; color: #93c5fd; border-color: #1e40af; }
    .pattern-cyclic_hub        { background: #2a0035; color: #e879f9; border-color: #86198f; }
    .pattern-hub_function      { background: #13104a; color: #a5b4fc; border-color: #3730a3; }
    .pattern-middle_man        { background: #1a2030; color: #94a3b8; border-color: #334155; }
    .pattern-neighbor_risk     { background: #002020; color: #5eead4; border-color: #0f766e; }
    .pattern-shotgun_target    { background: #3b0020; color: #f9a8d4; border-color: #9d174d; }
    .pattern-stale_complex     { background: #1a1200; color: #fde047; border-color: #854d0e; }
    .pattern-volatile_god      { background: #450a0a; color: #fef2f2; border-color: #7f1d1d; }

    /* Pattern breakdown widget — dark mode */
    .pattern-breakdown         { border-color: #374151; background: #1f2937; }
    .pattern-breakdown h2      { color: #f9fafb; }
    .pattern-breakdown-subtitle { color: #9ca3af; }
    .pattern-chip-desc         { color: #6b7280; }
    .pattern-chip-complex_branching { background: #2d1b00; }
    .pattern-chip-complex_branching .pattern-chip-count { color: #fbbf24; }
    .pattern-chip-deeply_nested     { background: #3a1500; }
    .pattern-chip-deeply_nested     .pattern-chip-count { color: #fb923c; }
    .pattern-chip-exit_heavy        { background: #1e0050; }
    .pattern-chip-exit_heavy        .pattern-chip-count { color: #c4b5fd; }
    .pattern-chip-god_function      { background: #3a0000; }
    .pattern-chip-god_function      .pattern-chip-count { color: #fca5a5; }
    .pattern-chip-long_function     { background: #3b0018; }
    .pattern-chip-long_function     .pattern-chip-count { color: #fda4af; }
    .pattern-chip-churn_magnet      { background: #001a3d; }
    .pattern-chip-churn_magnet      .pattern-chip-count { color: #93c5fd; }
    .pattern-chip-cyclic_hub        { background: #2a0035; }
    .pattern-chip-cyclic_hub        .pattern-chip-count { color: #e879f9; }
    .pattern-chip-hub_function      { background: #13104a; }
    .pattern-chip-hub_function      .pattern-chip-count { color: #a5b4fc; }
    .pattern-chip-middle_man        { background: #1a2030; }
    .pattern-chip-middle_man        .pattern-chip-count { color: #94a3b8; }
    .pattern-chip-neighbor_risk     { background: #002020; }
    .pattern-chip-neighbor_risk     .pattern-chip-count { color: #5eead4; }
    .pattern-chip-shotgun_target    { background: #3b0020; }
    .pattern-chip-shotgun_target    .pattern-chip-count { color: #f9a8d4; }
    .pattern-chip-stale_complex     { background: #1a1200; }
    .pattern-chip-stale_complex     .pattern-chip-count { color: #fde047; }
    .pattern-chip-volatile_god      { background: #450a0a; }
    .pattern-chip-volatile_god      .pattern-chip-count { color: #fef2f2; }
}
"#
}

/// Inline JavaScript for interactivity
fn inline_javascript() -> &'static str {
    r#"
(function() {
    let sortColumn = 'lrs';
    let sortDirection = 'desc';
    let currentPage = 1;
    let pageSize = 50;

    // Expose page navigation globally for inline onclick handlers
    window.__hsGoToPage = function(page) { currentPage = page; paginateTable(); };
    window.__hsChangePageSize = function(size) { pageSize = parseInt(size, 10); currentPage = 1; paginateTable(); };

    function sortTable(column) {
        const tbody = document.querySelector('#functions-table tbody');
        const rows = Array.from(tbody.querySelectorAll('tr'));

        if (sortColumn === column) {
            sortDirection = sortDirection === 'asc' ? 'desc' : 'asc';
        } else {
            sortColumn = column;
            sortDirection = 'desc';
        }

        document.querySelectorAll('th.sortable').forEach(th => th.classList.remove('asc', 'desc'));
        const activeHeader = document.querySelector(`th[data-column="${column}"]`);
        if (activeHeader) activeHeader.classList.add(sortDirection);

        rows.sort((a, b) => {
            let aVal = a.dataset[column] || '';
            let bVal = b.dataset[column] || '';
            const aNum = parseFloat(aVal);
            const bNum = parseFloat(bVal);
            if (!isNaN(aNum) && !isNaN(bNum)) return sortDirection === 'asc' ? aNum - bNum : bNum - aNum;
            return sortDirection === 'asc' ? aVal.localeCompare(bVal) : bVal.localeCompare(aVal);
        });

        rows.forEach(row => tbody.appendChild(row));
        currentPage = 1;
        paginateTable();
    }

    function filterTable() {
        const bandFilter = document.getElementById('band-filter').value;
        const driverFilter = document.getElementById('driver-filter').value;
        const searchFilter = document.getElementById('search-filter').value.toLowerCase();

        document.querySelectorAll('#functions-table tbody tr').forEach(row => {
            const bandMatch = bandFilter === 'all' || row.dataset.band === bandFilter;
            const driverMatch = driverFilter === 'all' || row.dataset.driver === driverFilter;
            const searchMatch = !searchFilter ||
                row.dataset.function.toLowerCase().includes(searchFilter) ||
                row.dataset.file.toLowerCase().includes(searchFilter);
            row.dataset.filterMatch = (bandMatch && driverMatch && searchMatch) ? '1' : '0';
        });

        currentPage = 1;
        paginateTable();
    }

    function paginateTable() {
        const rows = Array.from(document.querySelectorAll('#functions-table tbody tr'));
        const matched = rows.filter(r => r.dataset.filterMatch !== '0');
        const total = matched.length;
        const totalPages = Math.max(1, Math.ceil(total / pageSize));
        if (currentPage > totalPages) currentPage = totalPages;

        const start = (currentPage - 1) * pageSize;
        const end = start + pageSize;

        // Hide all, then show only the current page slice of matched rows
        rows.forEach(r => { r.style.display = 'none'; });
        matched.forEach((r, i) => { r.style.display = (i >= start && i < end) ? '' : 'none'; });

        const countEl = document.getElementById('visible-count');
        if (countEl) countEl.textContent = total;

        renderPaginationControls(currentPage, totalPages, total, start, end);
    }

    function renderPaginationControls(page, totalPages, total, start, end) {
        const el = document.getElementById('pagination-controls');
        if (!el) return;

        const from = total === 0 ? 0 : start + 1;
        const to = Math.min(end, total);
        const info = `Showing ${from}–${to} of ${total}`;

        // Build page number buttons with ellipsis for large page counts
        let pageButtons = '';
        if (totalPages <= 7) {
            for (let i = 1; i <= totalPages; i++) {
                pageButtons += `<button onclick="window.__hsGoToPage(${i})" class="page-btn${i === page ? ' active' : ''}" ${i === page ? 'disabled' : ''}>${i}</button>`;
            }
        } else {
            const visible = new Set([1, 2, page - 1, page, page + 1, totalPages - 1, totalPages].filter(p => p >= 1 && p <= totalPages));
            const sorted = Array.from(visible).sort((a, b) => a - b);
            let prev = 0;
            for (const p of sorted) {
                if (prev && p - prev > 1) pageButtons += '<span class="page-ellipsis">…</span>';
                pageButtons += `<button onclick="window.__hsGoToPage(${p})" class="page-btn${p === page ? ' active' : ''}" ${p === page ? 'disabled' : ''}>${p}</button>`;
                prev = p;
            }
        }

        const sizeOpts = [25, 50, 100, 9999].map(n =>
            `<option value="${n}" ${pageSize === n ? 'selected' : ''}>${n === 9999 ? 'All' : n + ' / page'}</option>`
        ).join('');

        el.innerHTML =
            `<span class="page-info">${info}</span>` +
            `<div class="page-buttons">` +
            `<button class="page-nav" onclick="window.__hsGoToPage(${page - 1})" ${page <= 1 ? 'disabled' : ''}>&larr; Prev</button>` +
            pageButtons +
            `<button class="page-nav" onclick="window.__hsGoToPage(${page + 1})" ${page >= totalPages ? 'disabled' : ''}>Next &rarr;</button>` +
            `</div>` +
            `<select class="page-size-select" onchange="window.__hsChangePageSize(this.value)">${sizeOpts}</select>`;
    }

    document.addEventListener('DOMContentLoaded', function() {
        document.querySelectorAll('th.sortable').forEach(th => {
            th.addEventListener('click', function() { sortTable(this.dataset.column); });
        });

        const bandFilter = document.getElementById('band-filter');
        const driverFilter = document.getElementById('driver-filter');
        const searchFilter = document.getElementById('search-filter');
        if (bandFilter) bandFilter.addEventListener('change', filterTable);
        if (driverFilter) driverFilter.addEventListener('change', filterTable);
        if (searchFilter) searchFilter.addEventListener('input', filterTable);

        // Mark all rows as filter-matching before first sort
        document.querySelectorAll('#functions-table tbody tr').forEach(r => { r.dataset.filterMatch = '1'; });

        // Sort by activity risk when available (richer signal), otherwise fall back to LRS
        const hasActivityCol = document.querySelector('th[data-column="activity"]');
        sortTable(hasActivityCol ? 'activity' : 'lrs');
    });

    // Trend charts — reads window.__hsHistory set by the inline script in the Trends section
    ;(function() {
        var hist = window.__hsHistory;
        if (!hist || hist.length < 2) return;
        var hoverIdx = -1, bandRaf = null;

        function isDark() { return !!(window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches); }

        function drawBandChart() {
            var el = document.getElementById('hs-bands-chart');
            if (!el) return;
            el.width = el.offsetWidth || 800;
            var ctx = el.getContext('2d'), W = el.width, H = el.height, N = hist.length;
            var lP = 48, rP = 8, tP = 12;
            var cW = W - lP - rP, cH = H - tP - 28;
            var dark = isDark(), fg = dark ? '#9ca3af' : '#6b7280', grd = dark ? '#374151' : '#e5e7eb';
            var mx = 1, i, t, b;
            for (i = 0; i < N; i++) {
                var s = (hist[i].critical||0) + (hist[i].high||0) + (hist[i].moderate||0) + (hist[i].low||0);
                if (s > mx) mx = s;
            }
            var bW = cW / N, gap = Math.max(1, bW * 0.12);
            var cols = ['#22c55e', '#eab308', '#f97316', '#ef4444'];
            ctx.clearRect(0, 0, W, H);
            ctx.font = '10px system-ui,sans-serif';
            for (t = 0; t <= 4; t++) {
                var yv = mx * t / 4, yp = tP + cH - (t / 4) * cH;
                ctx.fillStyle = fg; ctx.textAlign = 'right';
                ctx.fillText(Math.round(yv), lP - 4, yp + 4);
                ctx.strokeStyle = grd; ctx.lineWidth = 0.5;
                ctx.beginPath(); ctx.moveTo(lP, yp); ctx.lineTo(lP + cW, yp); ctx.stroke();
            }
            for (i = 0; i < N; i++) {
                var h = hist[i];
                var vs = [h.low||0, h.moderate||0, h.high||0, h.critical||0];
                var bx = lP + i * bW + gap / 2, bwi = bW - gap, yb = tP + cH;
                ctx.globalAlpha = (hoverIdx === i) ? 0.5 : 1.0;
                for (b = 0; b < 4; b++) {
                    var bh = (vs[b] / mx) * cH;
                    if (bh > 0) { ctx.fillStyle = cols[b]; ctx.fillRect(bx, yb - bh, bwi, bh); yb -= bh; }
                }
                ctx.globalAlpha = 1.0;
                if (i % Math.ceil(N / 6) === 0) {
                    var d = new Date(h.ts * 1000);
                    ctx.fillStyle = fg; ctx.textAlign = 'center'; ctx.font = '9px system-ui,sans-serif';
                    ctx.fillText((d.getUTCMonth() + 1) + '/' + d.getUTCDate(), lP + i * bW + bW / 2, tP + cH + 16);
                }
            }
            if (hoverIdx >= 0 && hoverIdx < N) {
                var hh = hist[hoverIdx];
                var htx = lP + hoverIdx * bW + bW / 2;
                var lbl = hh.sha + ' · c:' + (hh.critical||0) + ' h:' + (hh.high||0) + ' m:' + (hh.moderate||0) + ' l:' + (hh.low||0);
                ctx.font = 'bold 10px system-ui,sans-serif';
                var tw = ctx.measureText(lbl).width + 14;
                var ttx = Math.min(Math.max(lP + tw / 2, htx), lP + cW - tw / 2);
                ctx.fillStyle = dark ? '#1f2937' : '#ffffff';
                ctx.strokeStyle = dark ? '#374151' : '#d1d5db'; ctx.lineWidth = 1;
                ctx.beginPath();
                if (ctx.roundRect) ctx.roundRect(ttx - tw / 2, tP + 3, tw, 20, 4);
                else ctx.rect(ttx - tw / 2, tP + 3, tw, 20);
                ctx.fill(); ctx.stroke();
                ctx.fillStyle = dark ? '#f9fafb' : '#111827'; ctx.textAlign = 'center';
                ctx.fillText(lbl, ttx, tP + 17);
            }
        }

        function drawLineChart(id, key, color) {
            var el = document.getElementById(id);
            if (!el) return;
            el.width = el.offsetWidth || 400;
            var ctx = el.getContext('2d'), W = el.width, H = el.height, N = hist.length;
            var lP = 52, rP = 8, tP = 12;
            var cW = W - lP - rP, cH = H - tP - 24;
            var dark = isDark(), fg = dark ? '#9ca3af' : '#6b7280', grd = dark ? '#374151' : '#e5e7eb';
            var vals = hist.map(function(h) { return (h[key] || 0); });
            var minV = Math.min.apply(null, vals), maxV = Math.max.apply(null, vals);
            var range = maxV - minV;
            if (range < 1e-9) { minV -= 1; maxV += 1; range = 2; }
            var decimals = range < 0.01 ? 4 : range < 0.1 ? 2 : 1;
            ctx.clearRect(0, 0, W, H);
            ctx.font = '10px system-ui,sans-serif';
            for (var t = 0; t <= 4; t++) {
                var yv = minV + range * t / 4, yp = tP + cH - (t / 4) * cH;
                ctx.fillStyle = fg; ctx.textAlign = 'right';
                ctx.fillText(yv.toFixed(decimals), lP - 4, yp + 4);
                ctx.strokeStyle = grd; ctx.lineWidth = 0.5;
                ctx.beginPath(); ctx.moveTo(lP, yp); ctx.lineTo(lP + cW, yp); ctx.stroke();
            }
            var pts = vals.map(function(v, i) {
                return { x: lP + (N > 1 ? i * cW / (N - 1) : 0), y: tP + cH - ((v - minV) / range) * cH };
            });
            ctx.beginPath();
            pts.forEach(function(p, i) { if (i === 0) ctx.moveTo(p.x, p.y); else ctx.lineTo(p.x, p.y); });
            ctx.lineTo(pts[N - 1].x, tP + cH); ctx.lineTo(pts[0].x, tP + cH); ctx.closePath();
            ctx.globalAlpha = 0.2; ctx.fillStyle = color; ctx.fill(); ctx.globalAlpha = 1.0;
            ctx.beginPath();
            pts.forEach(function(p, i) { if (i === 0) ctx.moveTo(p.x, p.y); else ctx.lineTo(p.x, p.y); });
            ctx.strokeStyle = color; ctx.lineWidth = 2; ctx.stroke();
            var skip = Math.ceil(N / 4);
            for (var i = 0; i < N; i++) {
                if (i % skip === 0) {
                    var d = new Date(hist[i].ts * 1000);
                    ctx.fillStyle = fg; ctx.textAlign = 'center'; ctx.font = '9px system-ui,sans-serif';
                    ctx.fillText((d.getUTCMonth() + 1) + '/' + d.getUTCDate(), pts[i].x, tP + cH + 16);
                }
            }
        }

        function drawAll() {
            drawBandChart();
            drawLineChart('hs-risk-chart', 'risk', '#3b82f6');
            drawLineChart('hs-share-chart', 'share', '#8b5cf6');
        }

        document.addEventListener('DOMContentLoaded', function() {
            var bc = document.getElementById('hs-bands-chart');
            if (!bc) return;
            drawAll();
            bc.addEventListener('mousemove', function(e) {
                var r = bc.getBoundingClientRect();
                var mx2 = (e.clientX - r.left) * (bc.width / r.width);
                var bW2 = (bc.width - 56) / hist.length;
                var idx = Math.floor((mx2 - 48) / bW2);
                if (idx < 0 || idx >= hist.length) idx = -1;
                if (idx !== hoverIdx) {
                    hoverIdx = idx;
                    if (bandRaf) cancelAnimationFrame(bandRaf);
                    bandRaf = requestAnimationFrame(drawBandChart);
                }
            });
            bc.addEventListener('mouseleave', function() {
                hoverIdx = -1;
                if (bandRaf) cancelAnimationFrame(bandRaf);
                bandRaf = requestAnimationFrame(drawBandChart);
            });
            window.addEventListener('resize', function() { drawAll(); });
        });
    })();
})();
"#
}

/// Render header section
fn render_header(commit: &CommitInfo) -> String {
    let branch = commit.branch.as_deref().unwrap_or("detached");

    format!(
        r#"<header>
    <h1>Hotspots Report</h1>
    <div class="meta">
        <span>Commit: <code class="monospace">{sha}</code></span> •
        <span>Branch: <strong>{branch}</strong></span> •
        <span>Timestamp: {timestamp}</span>
    </div>
</header>"#,
        sha = &commit.sha[..8],
        branch = branch,
        timestamp = format_timestamp(commit.timestamp),
    )
}

/// Render summary section
fn render_summary(snapshot: &Snapshot) -> String {
    let total_functions = snapshot.functions.len();
    let critical_count = snapshot
        .functions
        .iter()
        .filter(|f| f.band == "critical")
        .count();
    let high_count = snapshot
        .functions
        .iter()
        .filter(|f| f.band == "high")
        .count();
    let avg_lrs = if total_functions > 0 {
        snapshot.functions.iter().map(|f| f.lrs).sum::<f64>() / total_functions as f64
    } else {
        0.0
    };

    let mut extra = String::new();
    if let Some(summary) = &snapshot.summary {
        if summary.total_activity_risk > 0.0 {
            extra.push_str(&format!(
                r#"
    <div class="summary-card">
        <h3>Total Activity Risk</h3>
        <div class="value">{:.1}</div>
    </div>"#,
                summary.total_activity_risk
            ));
        }
        if summary.top_1_pct_share > 0.0 {
            extra.push_str(&format!(
                r#"
    <div class="summary-card">
        <h3>Top 1% Share</h3>
        <div class="value">{:.1}%</div>
    </div>"#,
                summary.top_1_pct_share * 100.0
            ));
        }
        if let Some(cg) = &summary.call_graph {
            if cg.total_edges > 0 {
                extra.push_str(&format!(
                    r#"
    <div class="summary-card">
        <h3>Call Graph Edges</h3>
        <div class="value">{}</div>
    </div>"#,
                    cg.total_edges
                ));
            }
        }
    }

    format!(
        r#"<div class="summary">
    <div class="summary-card">
        <h3>Total Functions</h3>
        <div class="value">{total}</div>
    </div>
    <div class="summary-card">
        <h3>Critical Risk</h3>
        <div class="value band-critical">{critical}</div>
    </div>
    <div class="summary-card">
        <h3>High Risk</h3>
        <div class="value band-high">{high}</div>
    </div>
    <div class="summary-card">
        <h3>Average LRS</h3>
        <div class="value">{avg:.2}</div>
    </div>{extra}
</div>"#,
        total = total_functions,
        critical = critical_count,
        high = high_count,
        avg = avg_lrs,
        extra = extra,
    )
}

/// Render pattern breakdown widget — shows per-pattern counts sorted by frequency.
/// Returns empty string when no functions have patterns.
fn render_pattern_breakdown(functions: &[FunctionSnapshot]) -> String {
    use std::collections::HashMap;
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for f in functions {
        for p in &f.patterns {
            *counts.entry(p.as_str()).or_insert(0) += 1;
        }
    }
    if counts.is_empty() {
        return String::new();
    }
    let mut sorted: Vec<(&str, usize)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

    let chips: String = sorted
        .iter()
        .map(|(id, count)| {
            let desc = pattern_description(id);
            format!(
                r#"<div class="pattern-chip pattern-chip-{id}"><div class="pattern-chip-count">{count}</div><div class="pattern-chip-name">{id}</div><div class="pattern-chip-desc">{desc}</div></div>"#,
                id = html_escape(id),
                count = count,
                desc = desc,
            )
        })
        .collect();

    let affected = functions.iter().filter(|f| !f.patterns.is_empty()).count();
    format!(
        r#"<div class="pattern-breakdown"><h2>Pattern Breakdown</h2><p class="pattern-breakdown-subtitle">Detected across {affected} function{s}</p><div class="pattern-chips">{chips}</div></div>"#,
        affected = affected,
        s = if affected == 1 { "" } else { "s" },
        chips = chips,
    )
}

fn pattern_description(id: &str) -> &'static str {
    match id {
        "complex_branching" => "High cyclomatic complexity and nesting",
        "deeply_nested" => "Nesting depth \u{2265} 5 levels",
        "exit_heavy" => "Many early returns",
        "god_function" => "Too many responsibilities",
        "long_function" => "Exceeds recommended length",
        "churn_magnet" => "Complex and frequently changed",
        "cyclic_hub" => "Node in a dependency cycle",
        "hub_function" => "High fan-in and complex",
        "middle_man" => "High fan-out, trivial logic",
        "neighbor_risk" => "Called from high-churn functions",
        "shotgun_target" => "Many callers and high churn",
        "stale_complex" => "Complex but rarely touched",
        "volatile_god" => "God function under heavy churn",
        _ => "",
    }
}

/// Render functions table
fn render_functions_table(functions: &[FunctionSnapshot]) -> String {
    // Only show churn/fanin columns when enough functions actually have data
    let sparse_min = 10usize;
    let has_activity = functions.iter().any(|f| f.activity_risk.is_some());
    let has_churn = functions.iter().filter(|f| f.churn.is_some()).count() >= sparse_min;
    let has_touches = functions.iter().any(|f| f.touch_count_30d.is_some());
    let has_recency = functions.iter().any(|f| f.days_since_last_change.is_some());
    let has_fanin = functions.iter().filter(|f| f.callgraph.is_some()).count() >= sparse_min;
    let has_patterns = functions.iter().any(|f| !f.patterns.is_empty());

    let rows: String = functions
        .iter()
        .map(|f| {
            let function_name = f.function_id.split("::").last().unwrap_or(&f.function_id);
            let driver_str = f.driver.as_deref().unwrap_or("");
            let driver_badge = if let Some(driver) = &f.driver {
                let title = f
                    .driver_detail
                    .as_ref()
                    .map(|d| format!(" title=\"{}\"", html_escape(d)))
                    .unwrap_or_default();
                format!(
                    r#" <span class="driver-badge driver-{}"{title}>{}</span>"#,
                    html_escape(driver),
                    html_escape(driver),
                    title = title,
                )
            } else {
                String::new()
            };

            let churn_val = f.churn.as_ref().map(|c| c.lines_added + c.lines_deleted);

            let activity_cell = if has_activity {
                match f.activity_risk {
                    Some(ar) => format!("<td>{:.2}</td>", ar),
                    None => "<td>—</td>".to_string(),
                }
            } else {
                String::new()
            };
            let churn_cell = if has_churn {
                match churn_val {
                    Some(c) => format!("<td>{}</td>", c),
                    None => "<td>—</td>".to_string(),
                }
            } else {
                String::new()
            };
            let touches_cell = if has_touches {
                match f.touch_count_30d {
                    Some(t) => format!("<td>{}</td>", t),
                    None => "<td>—</td>".to_string(),
                }
            } else {
                String::new()
            };
            let fanin_cell = if has_fanin {
                match f.callgraph.as_ref().map(|cg| cg.fan_in) {
                    Some(fi) => format!("<td>{}</td>", fi),
                    None => "<td>—</td>".to_string(),
                }
            } else {
                String::new()
            };
            let recency_cell = if has_recency {
                match f.days_since_last_change {
                    Some(d) if d <= 7 => {
                        format!("<td><span class=\"recency-hot\">{d}d</span></td>", d = d)
                    }
                    Some(d) if d <= 30 => {
                        format!("<td><span class=\"recency-warm\">{d}d</span></td>", d = d)
                    }
                    Some(d) if d <= 90 => {
                        format!("<td><span class=\"recency-cool\">{d}d</span></td>", d = d)
                    }
                    Some(d) => {
                        format!("<td><span class=\"recency-cold\">{d}d</span></td>", d = d)
                    }
                    None => "<td>—</td>".to_string(),
                }
            } else {
                String::new()
            };
            let patterns_cell = if has_patterns {
                if f.patterns.is_empty() {
                    "<td>—</td>".to_string()
                } else {
                    let spans: String = f
                        .patterns
                        .iter()
                        .enumerate()
                        .map(|(i, id)| {
                            let title = f
                                .pattern_details
                                .as_ref()
                                .and_then(|ds| ds.get(i))
                                .map(|d| {
                                    let conds = d
                                        .triggered_by
                                        .iter()
                                        .map(|t| {
                                            format!(
                                                "{}={} ({}{})",
                                                t.metric, t.value, t.op, t.threshold
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                        .join(", ");
                                    format!(" title=\"{}\"", html_escape(&conds))
                                })
                                .unwrap_or_default();
                            format!(
                                r#"<span class="pattern pattern-{}"{title}>{}</span>"#,
                                html_escape(id),
                                html_escape(id),
                                title = title,
                            )
                        })
                        .collect();
                    format!("<td><div class=\"pattern-cell\">{}</div></td>", spans)
                }
            } else {
                String::new()
            };

            format!(
                "<tr data-file=\"{file}\" data-function=\"{function}\" data-band=\"{band}\" \
                 data-lrs=\"{lrs}\" data-line=\"{line}\" data-cc=\"{cc}\" data-nd=\"{nd}\" \
                 data-driver=\"{driver}\" data-activity=\"{activity}\" data-churn=\"{churn}\" \
                 data-touches=\"{touches}\" data-fanin=\"{fanin}\" \
                 data-recency=\"{recency}\">\n\
                 <td class=\"monospace\">{file_display}</td>\n\
                 <td>{function_display}{driver_badge}</td>\n\
                 <td>{line}</td>\n\
                 <td>{lrs:.2}</td>\n\
                 <td><span class=\"band-{band}\">{band}</span></td>\n\
                 <td>{cc}</td>\n\
                 <td>{nd}</td>\n\
                 <td>{fo}</td>\n\
                 <td>{ns}</td>\n\
                 {activity_cell}{churn_cell}{touches_cell}{recency_cell}{fanin_cell}{patterns_cell}\
                 </tr>",
                file = html_escape(&f.file),
                file_display = html_escape(&f.file),
                function = html_escape(function_name),
                function_display = html_escape(function_name),
                driver = html_escape(driver_str),
                activity = f
                    .activity_risk
                    .map(|ar| format!("{:.4}", ar))
                    .unwrap_or_default(),
                churn = churn_val.map(|c| c.to_string()).unwrap_or_default(),
                touches = f.touch_count_30d.map(|t| t.to_string()).unwrap_or_default(),
                fanin = f
                    .callgraph
                    .as_ref()
                    .map(|cg| cg.fan_in.to_string())
                    .unwrap_or_default(),
                recency = f
                    .days_since_last_change
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
                line = f.line,
                lrs = f.lrs,
                band = &f.band,
                cc = f.metrics.cc,
                nd = f.metrics.nd,
                fo = f.metrics.fo,
                ns = f.metrics.ns,
                driver_badge = driver_badge,
                activity_cell = activity_cell,
                churn_cell = churn_cell,
                touches_cell = touches_cell,
                recency_cell = recency_cell,
                fanin_cell = fanin_cell,
                patterns_cell = patterns_cell,
            )
        })
        .collect();

    let activity_header = if has_activity {
        "<th class=\"sortable\" data-column=\"activity\" title=\"Combined risk score weighting complexity, recent churn, and call graph centrality\">Activity Risk</th>"
    } else {
        ""
    };
    let churn_header = if has_churn {
        "<th class=\"sortable\" data-column=\"churn\" title=\"Lines added + deleted in recent git history\">Churn</th>"
    } else {
        ""
    };
    let touches_header = if has_touches {
        "<th class=\"sortable\" data-column=\"touches\" title=\"Number of commits touching this function in the last 30 days\">Touches</th>"
    } else {
        ""
    };
    let recency_header = if has_recency {
        "<th class=\"sortable\" data-column=\"recency\" title=\"Days since this function was last modified\">Last Change</th>"
    } else {
        ""
    };
    let fanin_header = if has_fanin {
        "<th class=\"sortable\" data-column=\"fanin\" title=\"Number of functions that call this one — higher means more callers, riskier to change\">Fan-in</th>"
    } else {
        ""
    };
    let patterns_header = if has_patterns {
        "<th title=\"Detected structural and behavioral patterns\">Patterns</th>"
    } else {
        ""
    };

    format!(
        r#"<section class="section">
    <h2>Functions (<span id="visible-count">{count}</span> of {count})</h2>

    <div class="filters">
        <div class="filter-group">
            <label for="band-filter">Risk Band</label>
            <select id="band-filter">
                <option value="all">All Bands</option>
                <option value="critical">Critical</option>
                <option value="high">High</option>
                <option value="moderate">Moderate</option>
                <option value="low">Low</option>
            </select>
        </div>
        <div class="filter-group">
            <label for="driver-filter">Driver</label>
            <select id="driver-filter">
                <option value="all">All Drivers</option>
                <option value="high_complexity">High Complexity</option>
                <option value="deep_nesting">Deep Nesting</option>
                <option value="high_churn_low_cc">High Churn / Low CC</option>
                <option value="high_fanin_complex">High Fan-in</option>
                <option value="high_fanout_churning">High Fan-out</option>
                <option value="cyclic_dep">Cyclic Dep</option>
                <option value="composite">Composite</option>
            </select>
        </div>
        <div class="filter-group">
            <label for="search-filter">Search</label>
            <input type="text" id="search-filter" placeholder="Function or file name...">
        </div>
    </div>

    <div id="pagination-controls" class="pagination-controls"></div>

    <table id="functions-table">
        <thead>
            <tr>
                <th class="sortable" data-column="file">File</th>
                <th class="sortable" data-column="function">Function</th>
                <th class="sortable" data-column="line">Line</th>
                <th class="sortable" data-column="lrs" title="Local Risk Score — composite metric combining complexity, nesting depth, and other factors">LRS</th>
                <th class="sortable" data-column="band" title="Risk band based on LRS: low / moderate / high / critical">Band</th>
                <th class="sortable" data-column="cc" title="Cyclomatic Complexity — number of independent paths through the function (lower is better)">CC</th>
                <th class="sortable" data-column="nd" title="Nesting Depth — maximum level of nested control structures">ND</th>
                <th title="Fan-out — number of distinct functions called by this function">FO</th>
                <th title="Number of Statements">NS</th>
                {activity_header}
                {churn_header}
                {touches_header}
                {recency_header}
                {fanin_header}
                {patterns_header}
            </tr>
        </thead>
        <tbody>
            {rows}
        </tbody>
    </table>
</section>"#,
        count = functions.len(),
        rows = rows,
        activity_header = activity_header,
        churn_header = churn_header,
        touches_header = touches_header,
        recency_header = recency_header,
        fanin_header = fanin_header,
        patterns_header = patterns_header,
    )
}

/// Map (driver, quadrant) to a one-line recommended action for the triage table.
fn triage_action(driver: Option<&str>, quadrant: Option<&str>) -> &'static str {
    crate::snapshot::driver_action_for_quadrant(driver.unwrap_or(""), quadrant.unwrap_or(""))
}

/// Render triage panel: quadrant summary + top risks table
fn render_triage_panel(functions: &[FunctionSnapshot]) -> String {
    let has_high_risk = functions
        .iter()
        .any(|f| f.band == "critical" || f.band == "high");
    if !has_high_risk {
        return String::new();
    }

    let has_quadrant = functions.iter().any(|f| f.quadrant.is_some());

    // Quadrant counts — read from pre-computed field
    let fire = functions
        .iter()
        .filter(|f| f.quadrant.as_deref() == Some("fire"))
        .count();
    let debt = functions
        .iter()
        .filter(|f| f.quadrant.as_deref() == Some("debt"))
        .count();
    let watch = functions
        .iter()
        .filter(|f| f.quadrant.as_deref() == Some("watch"))
        .count();
    let ok = functions
        .iter()
        .filter(|f| f.quadrant.as_deref() == Some("ok"))
        .count();

    // Top risks: fire (active high/critical) first, then debt (stable high/critical)
    let mut active_risks: Vec<&FunctionSnapshot> = functions
        .iter()
        .filter(|f| f.quadrant.as_deref() == Some("fire"))
        .collect();
    active_risks.sort_by(|a, b| {
        b.activity_risk
            .unwrap_or(b.lrs)
            .partial_cmp(&a.activity_risk.unwrap_or(a.lrs))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut inactive_risks: Vec<&FunctionSnapshot> = functions
        .iter()
        .filter(|f| f.quadrant.as_deref() == Some("debt"))
        .collect();
    inactive_risks.sort_by(|a, b| {
        b.activity_risk
            .unwrap_or(b.lrs)
            .partial_cmp(&a.activity_risk.unwrap_or(a.lrs))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut top_risks = active_risks;
    top_risks.extend(inactive_risks);
    top_risks.truncate(15);

    let show_touches = top_risks.iter().any(|f| f.touch_count_30d.is_some());
    let show_last_change = top_risks.iter().any(|f| f.days_since_last_change.is_some());
    let count = top_risks.len();

    let rows: String = top_risks
        .into_iter()
        .map(|f| {
            let function_name = f.function_id.split("::").last().unwrap_or(&f.function_id);
            let risk_val = f.activity_risk.unwrap_or(f.lrs);

            let driver_cell = match &f.driver {
                Some(d) => {
                    let title = f
                        .driver_detail
                        .as_ref()
                        .map(|dd| format!(" title=\"{}\"", html_escape(dd)))
                        .unwrap_or_default();
                    format!(
                        r#"<span class="driver-badge driver-{}"{title}>{}</span>"#,
                        html_escape(d),
                        html_escape(d),
                        title = title,
                    )
                }
                None => "—".to_string(),
            };

            let touches_td = if show_touches {
                let inner = match f.touch_count_30d {
                    Some(t) if t > 5 => {
                        format!(r#"<span class="recency-hot">{}</span>"#, t)
                    }
                    Some(t) if t > 0 => {
                        format!(r#"<span class="recency-warm">{}</span>"#, t)
                    }
                    Some(t) => t.to_string(),
                    None => "—".to_string(),
                };
                format!("<td>{}</td>", inner)
            } else {
                String::new()
            };

            let last_change_td = if show_last_change {
                let inner = match f.days_since_last_change {
                    Some(d) if d <= 7 => {
                        format!(r#"<span class="recency-hot">{d}d</span>"#, d = d)
                    }
                    Some(d) if d <= 30 => {
                        format!(r#"<span class="recency-warm">{d}d</span>"#, d = d)
                    }
                    Some(d) if d <= 90 => {
                        format!(r#"<span class="recency-cool">{d}d</span>"#, d = d)
                    }
                    Some(d) => format!(r#"<span class="recency-cold">{d}d</span>"#, d = d),
                    None => "—".to_string(),
                };
                format!("<td>{}</td>", inner)
            } else {
                String::new()
            };

            let fanin_td = match &f.callgraph {
                Some(cg) if cg.fan_in <= 2 => format!(
                    r#"<td><span class="refactor-ready">{} (safe)</span></td>"#,
                    cg.fan_in
                ),
                Some(cg) => format!("<td>{}</td>", cg.fan_in),
                None => "<td>—</td>".to_string(),
            };

            let row_class = if f.quadrant.as_deref() == Some("fire") {
                " class=\"triage-active-row\""
            } else {
                ""
            };

            format!(
                "<tr{cls}>\n\
                 <td class=\"monospace\">{file}</td>\n\
                 <td>{func}</td>\n\
                 <td><span class=\"band-{band}\">{band}</span></td>\n\
                 <td>{risk:.2}</td>\n\
                 <td>{driver}</td>\n\
                 {touches_td}{last_change_td}{fanin_td}\
                 <td class=\"triage-action\">{action}</td>\n\
                 </tr>",
                cls = row_class,
                file = html_escape(&f.file),
                func = html_escape(function_name),
                band = &f.band,
                risk = risk_val,
                driver = driver_cell,
                touches_td = touches_td,
                last_change_td = last_change_td,
                fanin_td = fanin_td,
                action = triage_action(f.driver.as_deref(), f.quadrant.as_deref()),
            )
        })
        .collect();

    let chips_html = if has_quadrant {
        format!(
            r#"<div class="quadrant-chips">
    <div class="quadrant-chip quadrant-fire">
        <div class="chip-label">Active Risk</div>
        <div class="chip-count">{fire}</div>
        <div class="chip-desc">high/critical + recently active</div>
    </div>
    <div class="quadrant-chip quadrant-debt">
        <div class="chip-label">Stable Debt</div>
        <div class="chip-count">{debt}</div>
        <div class="chip-desc">high/critical + not recently active</div>
    </div>
    <div class="quadrant-chip quadrant-watch">
        <div class="chip-label">Watch</div>
        <div class="chip-count">{watch}</div>
        <div class="chip-desc">moderate/low + recently active</div>
    </div>
    <div class="quadrant-chip quadrant-ok">
        <div class="chip-label">OK</div>
        <div class="chip-count">{ok}</div>
        <div class="chip-desc">low risk, not recently active</div>
    </div>
</div>"#,
            fire = fire,
            debt = debt,
            watch = watch,
            ok = ok,
        )
    } else {
        String::new()
    };

    let touches_th = if show_touches {
        "<th>Touches (30d)</th>"
    } else {
        ""
    };
    let last_change_th = if show_last_change {
        "<th>Last Change</th>"
    } else {
        ""
    };

    format!(
        r#"<section class="section triage-section" id="triage">
    <h2>Triage</h2>
    {chips}
    <h3 class="triage-subtitle">Top Risks ({count})</h3>
    <table>
        <thead>
            <tr>
                <th>File</th>
                <th>Function</th>
                <th>Band</th>
                <th>Risk</th>
                <th>Driver</th>
                {touches_th}
                {last_change_th}
                <th>Fan-in</th>
                <th>Action</th>
            </tr>
        </thead>
        <tbody>
            {rows}
        </tbody>
    </table>
</section>"#,
        chips = chips_html,
        count = count,
        touches_th = touches_th,
        last_change_th = last_change_th,
        rows = rows,
    )
}

/// Render aggregates section (File Risk, Module Instability, Co-change Coupling)
fn render_aggregates(aggregates: &SnapshotAggregates) -> String {
    let mut sections = Vec::new();

    // 4a. File Risk Table (joined with FileAggregates for LRS metrics)
    if !aggregates.file_risk.is_empty() {
        let show_file_churn = aggregates.file_risk.iter().any(|f| f.file_churn > 0);

        // Build a lookup: file -> (sum_lrs, max_lrs, high_plus_count)
        let lrs_lookup: std::collections::HashMap<&str, &crate::aggregates::FileAggregates> =
            aggregates
                .files
                .iter()
                .map(|fa| (fa.file.as_str(), fa))
                .collect();

        let rows: String = aggregates
            .file_risk
            .iter()
            .take(30)
            .map(|f| {
                let lrs = lrs_lookup.get(f.file.as_str());
                let sum_lrs = lrs.map(|l| l.sum_lrs).unwrap_or(0.0);
                let max_lrs = lrs.map(|l| l.max_lrs).unwrap_or(0.0);
                let high_plus = lrs.map(|l| l.high_plus_count).unwrap_or(0);
                let churn_td = if show_file_churn {
                    format!("<td>{}</td>", f.file_churn)
                } else {
                    String::new()
                };
                format!(
                    "<tr>\n\
                     <td class=\"monospace\">{file}</td>\n\
                     <td>{fns}</td>\n\
                     <td>{loc}</td>\n\
                     <td>{max_cc}</td>\n\
                     <td>{avg_cc:.1}</td>\n\
                     <td>{critical}</td>\n\
                     <td>{high_plus}</td>\n\
                     <td>{sum_lrs:.2}</td>\n\
                     <td>{max_lrs:.2}</td>\n\
                     {churn_td}\
                     <td>{score:.2}</td>\n\
                     </tr>",
                    file = html_escape(&f.file),
                    fns = f.function_count,
                    loc = f.loc,
                    max_cc = f.max_cc,
                    avg_cc = f.avg_cc,
                    critical = f.critical_count,
                    high_plus = high_plus,
                    sum_lrs = sum_lrs,
                    max_lrs = max_lrs,
                    churn_td = churn_td,
                    score = f.file_risk_score,
                )
            })
            .collect();

        let churn_th = if show_file_churn {
            "<th title=\"Total lines added + deleted across all recent commits\">Churn (lines)</th>"
        } else {
            ""
        };
        sections.push(format!(
            r#"<section class="section">
    <h2>File Risk (Top 30 by Risk Score)</h2>
    <table>
        <thead>
            <tr>
                <th>File</th>
                <th title="Number of functions in this file">Fns</th>
                <th title="Lines of code">LOC</th>
                <th title="Highest Cyclomatic Complexity of any function in the file">Max CC</th>
                <th title="Average Cyclomatic Complexity across all functions">Avg CC</th>
                <th title="Functions in the critical risk band">Critical</th>
                <th title="Functions in the high or critical risk band">High+</th>
                <th title="Sum of Local Risk Scores across all functions">Sum LRS</th>
                <th title="Highest single-function Local Risk Score in the file">Max LRS</th>
                {churn_th}
                <th title="Composite file risk: max_cc×0.4 + avg_cc×0.3 + log2(fns+1)×0.2 + churn_factor×0.1">Risk Score</th>
            </tr>
        </thead>
        <tbody>
            {rows}
        </tbody>
    </table>
</section>"#,
            churn_th = churn_th,
            rows = rows,
        ));
    }

    // 4b. Module Instability Table
    if !aggregates.modules.is_empty() {
        let rows: String = aggregates
            .modules
            .iter()
            .map(|m| {
                // Classify into Martin's zones using instability + complexity
                let (zone_label, zone_class) = if m.instability < 0.3 {
                    if m.avg_complexity > 8.0 {
                        ("zone of pain", "zone-pain")
                    } else {
                        ("stable", "zone-stable")
                    }
                } else if m.instability > 0.7 {
                    if m.avg_complexity > 8.0 {
                        ("volatile", "zone-volatile-complex")
                    } else {
                        ("volatile", "zone-volatile")
                    }
                } else {
                    ("balanced", "zone-balanced")
                };
                format!(
                    "<tr>\n\
                     <td class=\"monospace\">{module}</td>\n\
                     <td>{files}</td>\n\
                     <td>{fns}</td>\n\
                     <td>{avg_cc:.1}</td>\n\
                     <td>{afferent}</td>\n\
                     <td>{efferent}</td>\n\
                     <td>{instability:.2}</td>\n\
                     <td class=\"{zone_class}\">{zone_label}</td>\n\
                     </tr>",
                    module = html_escape(&m.module),
                    files = m.file_count,
                    fns = m.function_count,
                    avg_cc = m.avg_complexity,
                    afferent = m.afferent,
                    efferent = m.efferent,
                    instability = m.instability,
                    zone_class = zone_class,
                    zone_label = zone_label,
                )
            })
            .collect();

        sections.push(format!(
            r#"<section class="section">
    <h2>Module Instability</h2>
    <table>
        <thead>
            <tr>
                <th>Module</th>
                <th>Files</th>
                <th>Fns</th>
                <th title="Average Cyclomatic Complexity across all functions in the module">Avg CC</th>
                <th title="Afferent coupling — modules that depend on this one (higher = more depended upon)">Afferent</th>
                <th title="Efferent coupling — modules this one depends on (higher = more dependencies)">Efferent</th>
                <th title="I = Ce / (Ca + Ce). 0 = maximally stable, 1 = maximally unstable">Instability</th>
                <th title="Zone of Pain: stable but complex (I&lt;0.3, high CC). Volatile: changes often (I&gt;0.7). Balanced: healthy range.">Zone</th>
            </tr>
        </thead>
        <tbody>
            {rows}
        </tbody>
    </table>
</section>"#,
            rows = rows,
        ));
    }

    // 4c. Co-change Coupling Table — source files only, minimum 5 co-changes
    let qualifying: Vec<_> = aggregates
        .co_change
        .iter()
        .filter(|p| {
            (p.risk == "high" || p.risk == "moderate")
                && p.co_change_count >= 5
                && looks_like_source_file(&p.file_a)
                && looks_like_source_file(&p.file_b)
        })
        .take(20)
        .collect();

    if !qualifying.is_empty() {
        let show_static_dep = qualifying.iter().any(|p| p.has_static_dep);
        let rows: String = qualifying
            .iter()
            .map(|p| {
                let risk_class = match p.risk.as_str() {
                    "high" => " class=\"co-change-high\"",
                    "moderate" => " class=\"co-change-moderate\"",
                    _ => "",
                };
                let dep_td = if show_static_dep {
                    format!("<td>{}</td>", if p.has_static_dep { "yes" } else { "no" })
                } else {
                    String::new()
                };
                format!(
                    "<tr>\n\
                     <td class=\"monospace\">{file_a}</td>\n\
                     <td class=\"monospace\">{file_b}</td>\n\
                     <td>{count}</td>\n\
                     <td>{ratio:.0}%</td>\n\
                     <td{risk_class}>{risk}</td>\n\
                     {dep_td}\
                     </tr>",
                    file_a = html_escape(&p.file_a),
                    file_b = html_escape(&p.file_b),
                    count = p.co_change_count,
                    ratio = p.coupling_ratio * 100.0,
                    risk_class = risk_class,
                    risk = html_escape(&p.risk),
                    dep_td = dep_td,
                )
            })
            .collect();

        let dep_th = if show_static_dep {
            "<th title=\"Whether a direct import relationship exists between the two files\">Has Static Dep?</th>"
        } else {
            ""
        };

        sections.push(format!(
            r#"<section class="section">
    <h2>Co-change Coupling</h2>
    <table>
        <thead>
            <tr>
                <th>File A</th>
                <th>File B</th>
                <th title="Number of commits where both files changed together">Co-changes</th>
                <th title="co_changes / min(total_changes_A, total_changes_B)">Coupling %</th>
                <th>Risk</th>
                {dep_th}
            </tr>
        </thead>
        <tbody>
            {rows}
        </tbody>
    </table>
</section>"#,
            dep_th = dep_th,
            rows = rows,
        ));
    }

    sections.join("\n")
}

/// Render delta header
fn render_delta_header(commit: &crate::delta::DeltaCommitInfo) -> String {
    format!(
        r#"<header>
    <h1>Hotspots Delta Report</h1>
    <div class="meta">
        <span>Commit: <code class="monospace">{sha}</code></span> •
        <span>Parent: <code class="monospace">{parent}</code></span>
    </div>
</header>"#,
        sha = &commit.sha[..8],
        parent = if commit.parent.is_empty() {
            "none"
        } else {
            &commit.parent[..8]
        },
    )
}

/// Render delta summary
fn render_delta_summary(delta: &Delta) -> String {
    let new_count = delta
        .deltas
        .iter()
        .filter(|d| d.status == FunctionStatus::New)
        .count();
    let modified_count = delta
        .deltas
        .iter()
        .filter(|d| d.status == FunctionStatus::Modified)
        .count();
    let deleted_count = delta
        .deltas
        .iter()
        .filter(|d| d.status == FunctionStatus::Deleted)
        .count();
    let regressions = delta
        .deltas
        .iter()
        .filter(|d| d.delta.as_ref().map(|dt| dt.lrs > 0.0).unwrap_or(false))
        .count();

    format!(
        r#"<div class="summary">
    <div class="summary-card">
        <h3>New Functions</h3>
        <div class="value">{new}</div>
    </div>
    <div class="summary-card">
        <h3>Modified</h3>
        <div class="value">{modified}</div>
    </div>
    <div class="summary-card">
        <h3>Deleted</h3>
        <div class="value">{deleted}</div>
    </div>
    <div class="summary-card">
        <h3>Regressions</h3>
        <div class="value band-high">{regressions}</div>
    </div>
</div>"#,
        new = new_count,
        modified = modified_count,
        deleted = deleted_count,
        regressions = regressions,
    )
}

/// Render policy section
fn render_policy_section(policy: &PolicyResults, delta: &Delta) -> String {
    let mut sections = Vec::new();

    // Blocking failures
    if !policy.failed.is_empty() {
        let rows: String = policy
            .failed
            .iter()
            .map(|result| {
                let function_id = result.function_id.as_deref().unwrap_or("N/A");
                format!(
                    r#"<tr>
    <td class="monospace">{function}</td>
    <td>{policy}</td>
    <td>{message}</td>
</tr>"#,
                    function = html_escape(function_id),
                    policy = result.id.as_str(),
                    message = html_escape(&result.message),
                )
            })
            .collect();

        sections.push(format!(
            r#"<div class="policy-failures">
    <h3>Blocking Failures ({count})</h3>
    <table>
        <thead>
            <tr>
                <th>Function</th>
                <th>Policy</th>
                <th>Message</th>
            </tr>
        </thead>
        <tbody>
            {rows}
        </tbody>
    </table>
</div>"#,
            count = policy.failed.len(),
            rows = rows,
        ));
    }

    // Group warnings by policy ID
    let watch_warnings: Vec<_> = policy
        .warnings
        .iter()
        .filter(|w| w.id == PolicyId::WatchThreshold)
        .collect();
    let attention_warnings: Vec<_> = policy
        .warnings
        .iter()
        .filter(|w| w.id == PolicyId::AttentionThreshold)
        .collect();
    let rapid_growth_warnings: Vec<_> = policy
        .warnings
        .iter()
        .filter(|w| w.id == PolicyId::RapidGrowth)
        .collect();

    if !watch_warnings.is_empty() {
        sections.push(render_warning_group("Watch Level", &watch_warnings, delta));
    }
    if !attention_warnings.is_empty() {
        sections.push(render_warning_group(
            "Attention Level",
            &attention_warnings,
            delta,
        ));
    }
    if !rapid_growth_warnings.is_empty() {
        sections.push(render_warning_group(
            "Rapid Growth",
            &rapid_growth_warnings,
            delta,
        ));
    }

    if sections.is_empty() {
        return String::new();
    }

    format!(
        r#"<section class="section policy-section">
    <h2>Policy Results</h2>
    {sections}
</section>"#,
        sections = sections.join("\n")
    )
}

fn render_warning_group(
    title: &str,
    warnings: &[&crate::policy::PolicyResult],
    delta: &Delta,
) -> String {
    let rows: String = warnings
        .iter()
        .map(|result| {
            let function_id = result.function_id.as_deref().unwrap_or("N/A");
            let entry = delta.deltas.iter().find(|e| e.function_id == function_id);
            let after_lrs = entry
                .and_then(|e| e.after.as_ref())
                .map(|a| format!("{:.2}", a.lrs))
                .unwrap_or_else(|| "N/A".to_string());

            format!(
                r#"<tr>
    <td class="monospace">{function}</td>
    <td>{lrs}</td>
    <td>{message}</td>
</tr>"#,
                function = html_escape(function_id),
                lrs = after_lrs,
                message = html_escape(&result.message),
            )
        })
        .collect();

    format!(
        r#"<div class="policy-warnings">
    <h3>{title} ({count})</h3>
    <table>
        <thead>
            <tr>
                <th>Function</th>
                <th>LRS</th>
                <th>Message</th>
            </tr>
        </thead>
        <tbody>
            {rows}
        </tbody>
    </table>
</div>"#,
        title = title,
        count = warnings.len(),
        rows = rows,
    )
}

/// Render delta table
fn render_delta_table(deltas: &[FunctionDeltaEntry]) -> String {
    let rows: String = deltas
        .iter()
        .map(|entry| {
            let function_name = entry
                .function_id
                .split("::")
                .last()
                .unwrap_or(&entry.function_id);
            let before_lrs = entry
                .before
                .as_ref()
                .map(|b| format!("{:.2}", b.lrs))
                .unwrap_or_else(|| "-".to_string());
            let after_lrs = entry
                .after
                .as_ref()
                .map(|a| format!("{:.2}", a.lrs))
                .unwrap_or_else(|| "-".to_string());
            let before_band = entry
                .before
                .as_ref()
                .map(|b| b.band.as_str())
                .unwrap_or("-");
            let after_band = entry.after.as_ref().map(|a| a.band.as_str()).unwrap_or("-");
            let delta_lrs = entry
                .delta
                .as_ref()
                .map(|d| format!("{:+.2}", d.lrs))
                .unwrap_or_else(|| "-".to_string());

            let transition = match (
                entry.before.as_ref().map(|b| &b.band),
                entry.after.as_ref().map(|a| &a.band),
            ) {
                (Some(b), Some(a)) if b != a => {
                    if a > b {
                        "↑"
                    } else {
                        "↓"
                    }
                }
                (Some(_), Some(_)) => "→",
                _ => "-",
            };

            let status_class = match entry.status {
                FunctionStatus::New => "status-new",
                FunctionStatus::Deleted => "status-deleted",
                FunctionStatus::Modified => {
                    if entry.delta.as_ref().map(|d| d.lrs > 0.0).unwrap_or(false) {
                        "status-regression"
                    } else {
                        ""
                    }
                }
                FunctionStatus::Unchanged => "",
            };

            let status_debug = format!("{:?}", entry.status);
            let status_lowercase = status_debug.to_lowercase();

            format!(
                r#"<tr class="{status_class}" data-status="{status}" data-lrs="{after_lrs_val}">
    <td class="monospace">{function}</td>
    <td>{before_lrs}</td>
    <td>{after_lrs}</td>
    <td><span class="band-{before_band}">{before_band}</span></td>
    <td><span class="band-{after_band}">{after_band}</span></td>
    <td>{delta_lrs}</td>
    <td>{transition}</td>
    <td>{status_display}</td>
</tr>"#,
                status_class = status_class,
                status = status_lowercase,
                function = html_escape(function_name),
                before_lrs = before_lrs,
                after_lrs = after_lrs,
                after_lrs_val = entry.after.as_ref().map(|a| a.lrs).unwrap_or(0.0),
                before_band = before_band,
                after_band = after_band,
                delta_lrs = delta_lrs,
                transition = transition,
                status_display = status_debug,
            )
        })
        .collect();

    format!(
        r#"<section class="section">
    <h2>Function Changes ({count})</h2>
    <table id="delta-table">
        <thead>
            <tr>
                <th>Function</th>
                <th>Before LRS</th>
                <th>After LRS</th>
                <th>Before Band</th>
                <th>After Band</th>
                <th>Δ LRS</th>
                <th>Trend</th>
                <th>Status</th>
            </tr>
        </thead>
        <tbody>
            {rows}
        </tbody>
    </table>
</section>"#,
        count = deltas.len(),
        rows = rows,
    )
}

/// Render footer
fn render_footer() -> String {
    r#"<footer>
    <p>Generated by Hotspots</p>
</footer>"#
        .to_string()
}

/// Format Unix timestamp as human-readable UTC string ("YYYY-MM-DD HH:MM UTC")
fn format_timestamp(timestamp: i64) -> String {
    let secs = if timestamp < 0 {
        0u64
    } else {
        timestamp as u64
    };
    let days = secs / 86400;
    let rem = secs % 86400;
    let h = rem / 3600;
    let m = (rem % 3600) / 60;
    let (year, month, day) = days_to_ymd(days);
    format!("{:04}-{:02}-{:02} {:02}:{:02} UTC", year, month, day, h, m)
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let diy = if is_leap_year(year) { 366 } else { 365 };
        if days < diy {
            break;
        }
        days -= diy;
        year += 1;
    }
    let month_days: [u64; 12] = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u64;
    for &dim in &month_days {
        if days < dim {
            break;
        }
        days -= dim;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap_year(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

/// Returns false for lock files, docs, config, scripts — not useful in coupling analysis
fn looks_like_source_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    let basename = lower.rsplit('/').next().unwrap_or(&lower);
    const NON_SOURCE_EXT: &[&str] = &[
        ".lock",
        ".md",
        ".toml",
        ".json",
        ".yaml",
        ".yml",
        ".sh",
        ".bash",
        ".zsh",
        ".txt",
        ".cfg",
        ".ini",
        ".xml",
        ".gradle",
        ".env",
        ".properties",
        ".gitignore",
        ".gitattributes",
    ];
    const NON_SOURCE_NAMES: &[&str] = &[
        "makefile",
        "dockerfile",
        "license",
        "changelog",
        "readme",
        ".gitignore",
        ".gitattributes",
        ".editorconfig",
        ".dockerignore",
        ".npmrc",
        ".nvmrc",
    ];
    if NON_SOURCE_EXT.iter().any(|ext| basename.ends_with(ext)) {
        return false;
    }
    !NON_SOURCE_NAMES.contains(&basename)
}

/// Escape HTML special characters
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
