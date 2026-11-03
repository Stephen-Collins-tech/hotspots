//! HTML report generation
//!
//! Generates self-contained HTML reports with embedded CSS and JavaScript.
//! Reports are interactive (sorting, filtering) and work offline.

use crate::aggregates::SnapshotAggregates;
use crate::delta::{Delta, FunctionDeltaEntry, FunctionStatus};
use crate::policy::PolicyResults;
use crate::risk::{RiskBand, RiskThresholds};
use crate::snapshot::{CommitInfo, FunctionSnapshot, Snapshot, SnapshotSummary};

/// Render a snapshot as an HTML report.
///
/// `source_url` — optional URL of the corresponding written analysis post (e.g. a
/// hotspots.dev blog post). When set, a banner linking to that post is shown below
/// the header. Pass `None` for local CLI and CI use where no post exists.
pub fn render_html_snapshot(
    snapshot: &Snapshot,
    history: &[(CommitInfo, SnapshotSummary)],
    source_url: Option<&str>,
    _thresholds: &RiskThresholds,
) -> String {
    let aggregates = snapshot.aggregates.as_ref();
    let history_json = render_history_json(history);
    let trends = if history_json == "[]" {
        String::new()
    } else {
        render_trends_section(&history_json)
    };
    let source_banner = render_source_banner(source_url);
    let scatter_json = render_scatter_json(&snapshot.functions);
    let scatter = render_scatter_section(&scatter_json);

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
        {source_banner}
        {scatter}
        {summary}
        {triage}
        {aggregates_section}
        {next_actions}
        {trends}
        {functions_table}
        {footer}
    </div>
    <script>{js}</script>
</body>
</html>"#,
        sha = &snapshot.commit.sha[..8],
        css = inline_css(),
        js = inline_javascript(),
        header = render_header(&snapshot.commit),
        source_banner = source_banner,
        summary = render_summary(snapshot),
        next_actions = render_next_actions(&snapshot.functions),
        scatter = scatter,
        trends = trends,
        triage = render_triage_panel(&snapshot.functions),
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

/// Serialize per-function scatter data for the Risk Landscape chart.
///
/// Emits a compact JSON array: `[{"n":"fn","f":"file","x":lrs,"y":churn,"b":"h"},…]`
/// y = touch_count, falling back to total churn lines, then 0.
/// `b` is a single letter: c=critical, h=high, m=moderate, l=low.
fn render_scatter_json(functions: &[FunctionSnapshot]) -> String {
    if functions.is_empty() {
        return "[]".to_string();
    }
    let entries: Vec<String> = functions
        .iter()
        .map(|f| {
            let y = f
                .touch_count
                .map(|t| t as f64)
                .or_else(|| {
                    f.churn
                        .as_ref()
                        .map(|c| (c.lines_added + c.lines_deleted) as f64)
                })
                .unwrap_or(0.0);
            let b = match f.band.as_str() {
                "critical" => "c",
                "high" => "h",
                "moderate" => "m",
                _ => "l",
            };
            let name = compact_source_label(&f.function_id)
                .replace('\\', "\\\\")
                .replace('"', "\\\"");
            let file = compact_source_label(&f.file)
                .replace('\\', "\\\\")
                .replace('"', "\\\"");
            format!(
                r#"{{"n":"{name}","f":"{file}","x":{x:.2},"y":{y:.2},"b":"{b}"}}"#,
                name = name,
                file = file,
                x = f.lrs,
                y = y,
                b = b,
            )
        })
        .collect();
    format!("[{}]", entries.join(","))
}

/// Render the Risk Landscape scatter chart section.
fn render_scatter_section(json: &str) -> String {
    format!(
        r#"<script>window.__hsScatter = {json};</script>
<section class="section landscape-section" id="landscape">
    <div class="landscape-heading">
        <div>
            <h2>Risk Landscape</h2>
            <div class="chart-label">Start here: high and far-right points are the functions most likely to need attention; top-right means active regression risk.</div>
        </div>
        <div class="landscape-kicker">Complexity x Recent Change</div>
    </div>
    <canvas id="hs-scatter-chart" height="430"></canvas>
    <div class="scatter-legend">
        <div class="scatter-legend-bands">
            <span class="scatter-dot band-critical">●</span><span class="scatter-legend-label">Critical</span>
            <span class="scatter-dot band-high">●</span><span class="scatter-legend-label">High</span>
            <span class="scatter-dot band-moderate">●</span><span class="scatter-legend-label">Moderate</span>
            <span class="scatter-dot band-low">●</span><span class="scatter-legend-label">Low</span>
        </div>
        <div class="scatter-legend-axes">
            <div class="scatter-axis-row">
                <span class="scatter-axis-key">X: LRS</span>
                <span class="scatter-axis-desc">complexity score — cyclomatic paths · nesting depth · fan-out · exits</span>
            </div>
            <div class="scatter-axis-row">
                <span class="scatter-axis-key">Y: Touches</span>
                <span class="scatter-axis-desc">commits to this function in the last year</span>
            </div>
        </div>
    </div>
</section>"#,
        json = json,
    )
}

/// Render a delta as an HTML report.
///
/// `source_url` — optional URL of the corresponding written analysis post. When set,
/// a banner linking to that post is shown below the header. Pass `None` for local
/// CLI and CI use where no post exists.
pub fn render_html_delta(delta: &Delta, source_url: Option<&str>) -> String {
    let commit_sha = &delta.commit.sha[..8];
    let source_banner = render_source_banner(source_url);

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
        {source_banner}
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
        source_banner = source_banner,
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

details.section > summary {
    cursor: pointer;
    list-style: none;
    color: #1f2937;
    font-size: 1.15rem;
    font-weight: 800;
}

details.section > summary::-webkit-details-marker {
    display: none;
}

details.section > summary::before {
    content: "Show";
    display: inline-block;
    margin-right: 0.65rem;
    padding: 0.16rem 0.48rem;
    border-radius: 999px;
    background: #e5e7eb;
    color: #4b5563;
    font-size: 0.72rem;
    font-weight: 700;
}

details.section[open] > summary {
    margin-bottom: 1rem;
}

details.section[open] > summary::before {
    content: "Hide";
}

.section-summary-note {
    display: block;
    margin-top: 0.25rem;
    color: #6b7280;
    font-size: 0.82rem;
    font-weight: 400;
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

/* Overview */
.overview-section {
    padding: 1rem;
    border: 1px solid #e5e7eb;
    border-radius: 0.5rem;
    background: #f9fafb;
}

.overview-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(18rem, 1fr));
    gap: 0.9rem;
}

.overview-panel {
    border: 1px solid #e5e7eb;
    border-radius: 0.5rem;
    background: #ffffff;
    padding: 0.9rem;
}

.overview-panel h3 {
    color: #111827;
    font-size: 0.95rem;
    margin-bottom: 0.65rem;
}

.overview-stacked {
    display: flex;
    height: 1rem;
    overflow: hidden;
    border-radius: 999px;
    background: #e5e7eb;
}

.overview-segment {
    min-width: 2px;
}

.overview-bars {
    display: grid;
    gap: 0.55rem;
}

.overview-bar-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 0.6rem;
    align-items: center;
}

.overview-bar-label {
    color: #111827;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.overview-bar-value {
    color: #6b7280;
    font-size: 0.78rem;
}

.overview-mini-bar {
    grid-column: 1 / -1;
    height: 0.5rem;
    border-radius: 999px;
    background: #e5e7eb;
    overflow: hidden;
}

.overview-mini-fill {
    height: 100%;
    border-radius: inherit;
    background: #2563eb;
}

.overview-mini-fill.band-critical { background: #ef4444; }
.overview-mini-fill.band-high { background: #f97316; }
.overview-mini-fill.band-moderate { background: #eab308; }
.overview-mini-fill.band-low { background: #22c55e; }

.overview-kicker {
    color: #6b7280;
    font-size: 0.8rem;
    margin-top: 0.6rem;
}

/* Visual report surfaces */
.visual-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(18rem, 1fr));
    gap: 0.75rem;
}

.visual-card {
    border: 1px solid #e5e7eb;
    border-radius: 0.5rem;
    background: #ffffff;
    padding: 0.85rem;
    min-width: 0;
}

.visual-card-title {
    color: #111827;
    font-weight: 700;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.visual-card-subtitle {
    color: #6b7280;
    font-size: 0.75rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.source-link {
    color: inherit;
    text-decoration: underline;
    text-decoration-color: #9ca3af;
    text-underline-offset: 2px;
}

.source-link:hover {
    color: #2563eb;
    text-decoration-color: currentColor;
}

.visual-metrics {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.35rem;
    margin-top: 0.65rem;
}

.visual-metric {
    border-radius: 0.375rem;
    background: #f9fafb;
    padding: 0.35rem;
}

.visual-metric span {
    display: block;
    color: #6b7280;
    font-size: 0.68rem;
}

.visual-metric strong {
    color: #111827;
    font-size: 0.9rem;
}

.visual-bar {
    height: 0.55rem;
    border-radius: 999px;
    background: #e5e7eb;
    overflow: hidden;
    margin-top: 0.65rem;
}

.visual-bar-fill {
    height: 100%;
    border-radius: inherit;
    background: #2563eb;
}

.visual-bar-fill.band-critical { background: #ef4444; }
.visual-bar-fill.band-high { background: #f97316; }
.visual-bar-fill.band-moderate { background: #eab308; }
.visual-bar-fill.band-low { background: #22c55e; }

.visual-note {
    color: #6b7280;
    font-size: 0.82rem;
    margin-bottom: 0.75rem;
}

.raw-data-details {
    margin-top: 0.9rem;
}

.raw-data-details summary {
    cursor: pointer;
    color: #6b7280;
    font-weight: 600;
}

.triage-risk-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(20rem, 1fr));
    gap: 0.75rem;
}

.triage-risk-card {
    border: 1px solid #e5e7eb;
    border-left: 4px solid #f97316;
    border-radius: 0.5rem;
    background: #ffffff;
    padding: 0.85rem;
}

.triage-risk-card.fire {
    border-left-color: #ef4444;
}

.coupling-card {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    gap: 0.65rem;
    align-items: center;
}

.coupling-link {
    color: #2563eb;
    font-weight: 700;
    text-align: center;
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

/* Source banner — links back to the written analysis post */
.source-banner {
    background: #eff6ff;
    border: 1px solid #bfdbfe;
    border-radius: 6px;
    padding: 0.6rem 1rem;
    margin-bottom: 1.5rem;
    font-size: 0.875rem;
    color: #1e40af;
}
.source-banner a {
    color: #1d4ed8;
    font-weight: 600;
}

/* Summary legend — band thresholds + activity risk formula */
.summary-legend {
    font-size: 0.8rem;
    color: #6b7280;
    margin-top: 0.5rem;
    margin-bottom: 1.5rem;
}

/* Metric legend bar — always visible, above the functions table */
.metric-legend {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem 0.75rem;
    font-size: 0.8rem;
    color: #6b7280;
    margin-bottom: 0.75rem;
}
.metric-legend-label {
    font-weight: 600;
    color: #374151;
}
.metric-pill {
    background: transparent;
    border: 1px solid #e5e7eb;
    border-radius: 4px;
    padding: 0.15rem 0.5rem;
    cursor: help;
    white-space: nowrap;
    color: #6b7280;
}
.metric-pill strong {
    font-family: 'Monaco', 'Courier New', monospace;
    color: #4b5563;
}

/* Triage zero-state note */
.triage-zero-note {
    font-size: 0.875rem;
    color: #6b7280;
    background: #f9fafb;
    border-left: 3px solid #8b5cf6;
    padding: 0.5rem 0.75rem;
    margin: 0.5rem 0 1rem;
    border-radius: 0 4px 4px 0;
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
footer a {
    color: #4b5563;
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

/* Next actions */
.next-actions-section {
    border: 1px solid #bfdbfe;
    border-radius: 0.5rem;
    padding: 1.25rem;
    background: #eff6ff;
    box-shadow: 0 10px 24px rgba(37, 99, 235, 0.12);
}

.next-actions-section h2 {
    color: #1e3a8a;
    margin-bottom: 0.35rem;
}

.next-actions-subtitle {
    color: #475569;
    font-size: 0.86rem;
    margin-bottom: 0.9rem;
}

.next-actions-list {
    display: grid;
    gap: 0.65rem;
}

.next-action {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 0.75rem;
    align-items: start;
    border: 1px solid #dbeafe;
    border-left: 4px solid #2563eb;
    border-radius: 0.5rem;
    background: #ffffff;
    padding: 0.8rem;
}

.next-action:first-child {
    border-left-width: 6px;
    box-shadow: 0 8px 18px rgba(15, 23, 42, 0.08);
}

.next-action-fire { border-left-color: #ef4444; }
.next-action-debt { border-left-color: #8b5cf6; }
.next-action-watch { border-left-color: #f59e0b; }

.next-action-rank {
    color: #1d4ed8;
    font-weight: 800;
    font-size: 0.82rem;
}

.next-action-title {
    color: #111827;
    font-weight: 800;
    overflow-wrap: anywhere;
}

.next-action-meta {
    color: #64748b;
    font-size: 0.76rem;
    margin-top: 0.15rem;
    overflow-wrap: anywhere;
}

.next-action-why {
    color: #374151;
    font-size: 0.82rem;
    margin-top: 0.35rem;
}

.next-action-score {
    display: grid;
    gap: 0.15rem;
    justify-items: end;
    color: #64748b;
    font-size: 0.72rem;
    white-space: nowrap;
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
.tier-chips {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.75rem;
    margin-bottom: 1.5rem;
}

@media (max-width: 768px) {
    .tier-chips {
        grid-template-columns: repeat(2, 1fr);
    }
}

.tier-chip {
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    border-left: 4px solid;
    text-align: center;
}

.tier-active   { border-left-color: #ef4444; background: #fef2f2; }
.tier-quiet     { border-left-color: #22c55e; background: #f0fdf4; }

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

.tier-active .chip-count  { color: #ef4444; }
.tier-quiet .chip-count    { color: #22c55e; }

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
.landscape-section {
    border: 1px solid #dbeafe;
    background: linear-gradient(180deg, #ffffff 0%, #f8fbff 100%);
    box-shadow: 0 16px 34px rgba(15, 23, 42, 0.08);
}
.landscape-heading {
    display:flex;
    align-items:flex-start;
    justify-content:space-between;
    gap:1rem;
    margin-bottom:0.65rem;
}
.landscape-kicker {
    flex-shrink:0;
    border:1px solid #bfdbfe;
    border-radius:999px;
    background:#eff6ff;
    color:#1d4ed8;
    font-size:0.72rem;
    font-weight:800;
    letter-spacing:0;
    padding:0.28rem 0.65rem;
}
#hs-scatter-chart {
    display:block;
    border:1px solid #e5e7eb;
    border-radius:0.5rem;
    background:#ffffff;
    width:100%;
}
.scatter-legend { display:flex; flex-wrap:wrap; align-items:flex-start; justify-content:space-between; gap:0.75rem 2rem; margin-top:0.75rem; font-size:0.8rem; color:#6b7280; }
.scatter-legend-bands { display:flex; align-items:center; gap:0.5rem; flex-shrink:0; }
.scatter-dot { font-size:1rem; line-height:1; }
.scatter-legend-label { color:#6b7280; margin-right:0.25rem; }
.scatter-legend-axes { display:flex; flex-direction:column; gap:0.2rem; }
.scatter-axis-row { display:flex; align-items:baseline; gap:0.5rem; }
.scatter-axis-key { font-weight:700; color:#374151; white-space:nowrap; min-width:5.5rem; }
.scatter-axis-desc { color:#9ca3af; }
.trends-charts { display:grid; grid-template-columns:1fr 1fr; gap:1rem; margin-top:1rem; }
@media (max-width:768px) {
    .trends-charts { grid-template-columns:1fr; }
    .landscape-heading { display:block; }
    .landscape-kicker { display:inline-block; margin-top:0.5rem; }
}
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

    .overview-section,
    .overview-panel {
        background: #111827;
        border-color: #374151;
    }

    .overview-panel h3,
    .overview-bar-label {
        color: #f9fafb;
    }

    .overview-stacked,
    .overview-mini-bar {
        background: #374151;
    }

    .visual-card,
    .triage-risk-card {
        background: #111827;
        border-color: #374151;
    }

    .visual-card-title,
    .visual-metric strong {
        color: #f9fafb;
    }

    .visual-metric {
        background: #1f2937;
    }

    .visual-bar {
        background: #374151;
    }

    footer {
        border-top-color: #374151;
    }
    footer a { color: #9ca3af; }

    .source-banner {
        background: #1e3a5f;
        border-color: #2563eb;
        color: #93c5fd;
    }
    .source-banner a { color: #60a5fa; }

    .summary-legend { color: #6b7280; }

    .metric-legend-label { color: #9ca3af; }
    .metric-pill {
        background: #1f2937;
        border-color: #374151;
        color: #d1d5db;
    }
    .metric-pill strong { color: #f9fafb; }

    .triage-zero-note {
        background: #1a1030;
        border-left-color: #7c3aed;
        color: #9ca3af;
    }


    .page-btn, .page-nav { background: #1f2937; border-color: #374151; color: #f9fafb; }
    .page-btn:hover:not(:disabled), .page-nav:hover:not(:disabled) { background: #374151; }
    .page-size-select { background: #1f2937; border-color: #374151; color: #f9fafb; }

    .triage-section { border-color: #92400e; background: #1c1500; }
    details.section > summary { color: #f9fafb; }
    details.section > summary::before { background: #374151; color: #d1d5db; }
    .section-summary-note { color: #9ca3af; }
    .triage-section h2,
    .triage-section > summary { color: #fbbf24; }
    .triage-subtitle { color: #d1d5db; }
    .tier-active   { background: #1a0000; }
    .tier-quiet     { background: #001a08; }
    .chip-label { color: #9ca3af; }
    .chip-desc  { color: #6b7280; }
    .triage-active-row { background: #1c1200; }
    .triage-active-row:hover { background: #2a1a00; }
    .recency-cold { color: #4b5563; }
    .triage-action { color: #9ca3af; }
    .next-actions-section {
        background: #0f172a;
        border-color: #1d4ed8;
    }
    .next-actions-section h2 { color: #93c5fd; }
    .next-actions-subtitle,
    .next-action-rank,
    .next-action-meta,
    .next-action-score { color: #9ca3af; }
    .next-action {
        background: #111827;
        border-color: #374151;
    }
    .next-action-title { color: #f9fafb; }
    .next-action-why { color: #d1d5db; }
    .landscape-section {
        background: #0f172a;
        border-color: #1f2937;
        box-shadow: none;
    }
    .landscape-kicker {
        background: #172554;
        border-color: #1d4ed8;
        color: #bfdbfe;
    }
    .trends-section canvas { background:#1f2937; }
    #hs-scatter-chart { background:#111827; border-color:#374151; }
    .scatter-legend { color:#9ca3af; }
    .scatter-legend-label { color:#9ca3af; }
    .scatter-axis-key { color:#e5e7eb; }
    .scatter-axis-desc { color:#6b7280; }
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
        const tierFilter = document.getElementById('tier-filter').value;
        const searchFilter = document.getElementById('search-filter').value.toLowerCase();

        document.querySelectorAll('#functions-table tbody tr').forEach(row => {
            const bandMatch = bandFilter === 'all' || row.dataset.band === bandFilter;
            const tierMatch = tierFilter === 'all' || row.dataset.tier === tierFilter;
            const searchMatch = !searchFilter ||
                row.dataset.function.toLowerCase().includes(searchFilter) ||
                row.dataset.file.toLowerCase().includes(searchFilter);
            row.dataset.filterMatch = (bandMatch && tierMatch && searchMatch) ? '1' : '0';
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
        const tierFilter = document.getElementById('tier-filter');
        const searchFilter = document.getElementById('search-filter');
        if (bandFilter) bandFilter.addEventListener('change', filterTable);
        if (tierFilter) tierFilter.addEventListener('change', filterTable);
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

    // Risk Landscape scatter chart — reads window.__hsScatter
    ;(function() {
        var pts = window.__hsScatter;
        if (!pts || pts.length === 0) return;
        var hoveredIdx = -1, scatterRaf = null;
        var bandColor = { c: '#ef4444', h: '#f97316', m: '#eab308', l: '#22c55e' };

        function isDarkSc() { return !!(window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches); }

        function drawScatter() {
            var el = document.getElementById('hs-scatter-chart');
            if (!el) return;
            el.width = el.offsetWidth || 800;
            var ctx = el.getContext('2d'), W = el.width, H = el.height;
            var lP = 64, rP = 24, tP = 28, bP = 46;
            var cW = W - lP - rP, cH = H - tP - bP;
            var dark = isDarkSc(), fg = dark ? '#9ca3af' : '#6b7280', grd = dark ? '#374151' : '#e5e7eb';
            var panel = dark ? '#111827' : '#ffffff';
            var hotFill = dark ? 'rgba(239,68,68,0.16)' : 'rgba(254,226,226,0.72)';
            var debtFill = dark ? 'rgba(249,115,22,0.12)' : 'rgba(255,237,213,0.58)';
            var activeFill = dark ? 'rgba(234,179,8,0.10)' : 'rgba(254,249,195,0.45)';
            var calmFill = dark ? 'rgba(34,197,94,0.08)' : 'rgba(220,252,231,0.38)';

            var xs = pts.map(function(p) { return p.x; });
            var ys = pts.map(function(p) { return p.y; });
            var maxX = (Math.max.apply(null, xs) || 1) * 1.08;
            var maxY = (Math.max.apply(null, ys) || 1) * 1.08;

            // medians for quadrant dividers
            var sxs = xs.slice().sort(function(a, b) { return a - b; });
            var sys = ys.slice().sort(function(a, b) { return a - b; });
            var medX = sxs[Math.floor(sxs.length / 2)];
            var medY = sys[Math.floor(sys.length / 2)];

            ctx.clearRect(0, 0, W, H);
            ctx.font = '10px system-ui,sans-serif';

            ctx.fillStyle = panel;
            ctx.fillRect(0, 0, W, H);

            // quadrant regions
            var qx = lP + (medX / maxX) * cW;
            var qy = tP + cH - (medY / maxY) * cH;
            ctx.fillStyle = calmFill; ctx.fillRect(lP, qy, qx - lP, tP + cH - qy);
            ctx.fillStyle = activeFill; ctx.fillRect(lP, tP, qx - lP, qy - tP);
            ctx.fillStyle = debtFill; ctx.fillRect(qx, qy, lP + cW - qx, tP + cH - qy);
            ctx.fillStyle = hotFill; ctx.fillRect(qx, tP, lP + cW - qx, qy - tP);

            // grid lines
            for (var t = 0; t <= 4; t++) {
                var xv = maxX * t / 4, xp = lP + (t / 4) * cW;
                var yv = maxY * t / 4, yp = tP + cH - (t / 4) * cH;
                ctx.fillStyle = fg; ctx.textAlign = 'center';
                ctx.fillText(xv.toFixed(1), xp, tP + cH + 16);
                ctx.fillStyle = fg; ctx.textAlign = 'right';
                ctx.fillText(Math.round(yv), lP - 4, yp + 4);
                ctx.strokeStyle = grd; ctx.lineWidth = 0.5;
                ctx.beginPath(); ctx.moveTo(lP, yp); ctx.lineTo(lP + cW, yp); ctx.stroke();
                ctx.beginPath(); ctx.moveTo(xp, tP); ctx.lineTo(xp, tP + cH); ctx.stroke();
            }

            // quadrant dividers at median
            ctx.strokeStyle = dark ? '#6b7280' : '#94a3b8'; ctx.lineWidth = 1;
            ctx.setLineDash([4, 4]);
            ctx.beginPath(); ctx.moveTo(qx, tP); ctx.lineTo(qx, tP + cH); ctx.stroke();
            ctx.beginPath(); ctx.moveTo(lP, qy); ctx.lineTo(lP + cW, qy); ctx.stroke();
            ctx.setLineDash([]);

            function quadrantLabel(text, x, y, align) {
                ctx.font = 'bold 11px system-ui,sans-serif';
                ctx.fillStyle = dark ? 'rgba(249,250,251,0.72)' : 'rgba(17,24,39,0.62)';
                ctx.textAlign = align || 'left';
                ctx.fillText(text, x, y);
            }
            quadrantLabel('low LRS, high activity', lP + 10, tP + 18, 'left');
            quadrantLabel('high LRS, high activity', lP + cW - 10, tP + 18, 'right');
            quadrantLabel('low LRS, low activity', lP + 10, tP + cH - 10, 'left');
            quadrantLabel('high LRS, low activity', lP + cW - 10, tP + cH - 10, 'right');

            // dots
            var r = Math.max(3, Math.min(7, Math.round(cW / Math.sqrt(pts.length) * 0.18)));
            pts.forEach(function(p, i) {
                var cx = lP + (p.x / maxX) * cW;
                var cy = tP + cH - (p.y / maxY) * cH;
                var col = bandColor[p.b] || '#6b7280';
                ctx.beginPath();
                ctx.arc(cx, cy, i === hoveredIdx ? r + 3 : r, 0, Math.PI * 2);
                ctx.fillStyle = col;
                ctx.globalAlpha = i === hoveredIdx ? 1.0 : (p.b === 'c' ? 0.88 : 0.62);
                ctx.fill();
                if (p.b === 'c' || i === hoveredIdx) {
                    ctx.strokeStyle = dark ? '#111827' : '#ffffff';
                    ctx.lineWidth = i === hoveredIdx ? 3 : 2;
                    ctx.stroke();
                }
                ctx.globalAlpha = 1.0;
            });

            var callouts = pts.slice().sort(function(a, b) {
                var ar = (a.x * 1.4) + (a.y * 2.2) + (a.b === 'c' ? 5 : a.b === 'h' ? 2 : 0);
                var br = (b.x * 1.4) + (b.y * 2.2) + (b.b === 'c' ? 5 : b.b === 'h' ? 2 : 0);
                return br - ar;
            }).slice(0, 3);
            callouts.forEach(function(p, idx) {
                var cx = lP + (p.x / maxX) * cW;
                var cy = tP + cH - (p.y / maxY) * cH;
                var shortName = String(p.n || '').split('::').pop();
                if (shortName.length > 24) shortName = shortName.slice(0, 21) + '...';
                var tx = Math.min(Math.max(cx + 12, lP + 72), lP + cW - 72);
                var ty = Math.max(tP + 18, cy - 16 - (idx * 4));
                ctx.strokeStyle = dark ? 'rgba(156,163,175,0.45)' : 'rgba(100,116,139,0.45)';
                ctx.lineWidth = 1;
                ctx.beginPath(); ctx.moveTo(cx + 5, cy - 5); ctx.lineTo(tx - 5, ty + 3); ctx.stroke();
                ctx.font = 'bold 10px system-ui,sans-serif';
                ctx.textAlign = 'left';
                ctx.fillStyle = dark ? '#f9fafb' : '#111827';
                ctx.fillText(shortName, tx, ty);
            });

            // tooltip
            if (hoveredIdx >= 0 && hoveredIdx < pts.length) {
                var hp = pts[hoveredIdx];
                var hcx = lP + (hp.x / maxX) * cW;
                var hcy = tP + cH - (hp.y / maxY) * cH;
                var label = hp.n + '  LRS:' + hp.x.toFixed(1) + '  Touches:' + hp.y.toFixed(0);
                ctx.font = 'bold 10px system-ui,sans-serif';
                var tw = ctx.measureText(label).width + 18;
                var ttx = Math.min(Math.max(hcx, lP + tw / 2), lP + cW - tw / 2);
                var tty = hcy - 14;
                if (tty - 20 < tP) tty = hcy + 26;
                ctx.fillStyle = dark ? '#1f2937' : '#ffffff';
                ctx.strokeStyle = dark ? '#374151' : '#d1d5db'; ctx.lineWidth = 1;
                ctx.beginPath();
                if (ctx.roundRect) ctx.roundRect(ttx - tw / 2, tty - 16, tw, 22, 4);
                else ctx.rect(ttx - tw / 2, tty - 16, tw, 22);
                ctx.fill(); ctx.stroke();
                ctx.fillStyle = dark ? '#f9fafb' : '#111827'; ctx.textAlign = 'center';
                ctx.fillText(label, ttx, tty);
                // file path beneath
                ctx.font = '9px system-ui,sans-serif';
                ctx.fillStyle = dark ? '#6b7280' : '#9ca3af';
                ctx.fillText(hp.f, ttx, tty + 14);
            }

            // axis labels
            ctx.globalAlpha = 1.0;
            ctx.fillStyle = fg; ctx.font = '10px system-ui,sans-serif';
            ctx.textAlign = 'center';
            ctx.fillText('Complexity (LRS)', lP + cW / 2, H - 4);
            ctx.save();
            ctx.translate(12, tP + cH / 2);
            ctx.rotate(-Math.PI / 2);
            ctx.textAlign = 'center';
            ctx.fillText('Change Frequency', 0, 0);
            ctx.restore();
        }

        document.addEventListener('DOMContentLoaded', function() {
            var el = document.getElementById('hs-scatter-chart');
            if (!el) return;
            drawScatter();
            el.addEventListener('mousemove', function(e) {
                var r2 = el.getBoundingClientRect();
                var mx = (e.clientX - r2.left) * (el.width / r2.width);
                var my = (e.clientY - r2.top) * (el.height / r2.height);
                var lP2 = 64, rP2 = 24, tP2 = 28, bP2 = 46;
                var cW2 = el.width - lP2 - rP2, cH2 = el.height - tP2 - bP2;
                var xs2 = pts.map(function(p) { return p.x; });
                var ys2 = pts.map(function(p) { return p.y; });
                var maxX2 = (Math.max.apply(null, xs2) || 1) * 1.08;
                var maxY2 = (Math.max.apply(null, ys2) || 1) * 1.08;
                var best = -1, bestD = 400;
                pts.forEach(function(p, i) {
                    var cx = lP2 + (p.x / maxX2) * cW2;
                    var cy = tP2 + cH2 - (p.y / maxY2) * cH2;
                    var d = (mx - cx) * (mx - cx) + (my - cy) * (my - cy);
                    if (d < bestD) { bestD = d; best = i; }
                });
                if (best !== hoveredIdx) {
                    hoveredIdx = best;
                    if (scatterRaf) cancelAnimationFrame(scatterRaf);
                    scatterRaf = requestAnimationFrame(drawScatter);
                }
            });
            el.addEventListener('mouseleave', function() {
                hoveredIdx = -1;
                if (scatterRaf) cancelAnimationFrame(scatterRaf);
                scatterRaf = requestAnimationFrame(drawScatter);
            });
            window.addEventListener('resize', function() {
                drawScatter();
            });
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
        .filter(|f| f.band == RiskBand::Critical)
        .count();
    let high_count = snapshot
        .functions
        .iter()
        .filter(|f| f.band == RiskBand::High)
        .count();
    let active_count = snapshot
        .functions
        .iter()
        .filter(|f| f.tier.as_deref() == Some("active"))
        .count();
    let quiet_count = snapshot
        .functions
        .iter()
        .filter(|f| f.tier.as_deref() == Some("quiet"))
        .count();

    format!(
        r#"<div class="summary">
    <div class="summary-card">
        <h3>Total Functions</h3>
        <div class="value">{total}</div>
    </div>
    <div class="summary-card">
        <h3>Active</h3>
        <div class="value band-critical">{active}</div>
    </div>
    <div class="summary-card">
        <h3>Quiet</h3>
        <div class="value">{quiet}</div>
    </div>
    <div class="summary-card">
        <h3>High+ Risk</h3>
        <div class="value">{high_plus}</div>
    </div>
</div>
"#,
        total = total_functions,
        active = active_count,
        quiet = quiet_count,
        high_plus = critical_count + high_count,
    )
}

/// Render functions table
fn render_function_risk_gallery(functions: &[FunctionSnapshot]) -> String {
    let max_lrs = functions
        .iter()
        .map(|f| f.lrs)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let mut sorted = functions.iter().collect::<Vec<_>>();
    sorted.sort_by(|a, b| {
        b.activity_risk
            .unwrap_or(b.lrs)
            .partial_cmp(&a.activity_risk.unwrap_or(a.lrs))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b.lrs
                    .partial_cmp(&a.lrs)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    let cards: String = sorted
        .into_iter()
        .take(48)
        .map(|f| {
            let function_name = f.function_id.split("::").last().unwrap_or(&f.function_id);
            let width = ((f.lrs / max_lrs) * 100.0).clamp(4.0, 100.0);
            let activity = f.activity_risk.unwrap_or(f.lrs);
            let touches = f
                .touch_count
                .map(|t| t.to_string())
                .unwrap_or_else(|| "—".to_string());
            format!(
                r#"<div class="visual-card" data-band="{band}">
    <div class="visual-card-title">{function}</div>
    <div class="visual-card-subtitle monospace">{source}</div>
    <div class="visual-bar"><div class="visual-bar-fill band-{band}" style="width:{width:.0}%"></div></div>
    <div class="visual-metrics">
        <div class="visual-metric"><span>LRS</span><strong>{lrs:.2}</strong></div>
        <div class="visual-metric"><span>Activity</span><strong>{activity:.2}</strong></div>
        <div class="visual-metric"><span>Touches</span><strong>{touches}</strong></div>
        <div class="visual-metric"><span>CC</span><strong>{cc}</strong></div>
    </div>
</div>"#,
                function = html_escape(function_name),
                source = source_link(
                    &f.file,
                    f.line,
                    &format!("{}:{}", compact_source_label(&f.file), f.line)
                ),
                band = f.band.as_str(),
                width = width,
                lrs = f.lrs,
                activity = activity,
                touches = touches,
                cc = f.metrics.cc,
            )
        })
        .collect();

    format!(
        r#"<div class="visual-note">Top function risks rendered as bars. Use the raw table below only for exact rows, sorting, and export-style inspection.</div>
<div class="visual-grid">{cards}</div>"#,
        cards = cards,
    )
}

fn render_functions_table(functions: &[FunctionSnapshot]) -> String {
    // Only show churn/fanin columns when enough functions actually have data
    let sparse_min = 10usize;
    let has_activity = functions.iter().any(|f| f.activity_risk.is_some());
    let has_churn = functions.iter().filter(|f| f.churn.is_some()).count() >= sparse_min;
    let has_touches = functions.iter().any(|f| f.touch_count.is_some());
    let has_recency = functions.iter().any(|f| f.days_since_last_change.is_some());
    let has_fanin = functions.iter().filter(|f| f.callgraph.is_some()).count() >= sparse_min;

    let rows: String = functions
        .iter()
        .map(|f| {
            let function_name = f.function_id.split("::").last().unwrap_or(&f.function_id);
            let tier_str = f.tier.as_deref().unwrap_or("");

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
                match f.touch_count {
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

            format!(
                "<tr data-file=\"{file}\" data-function=\"{function}\" data-band=\"{band}\" \
                 data-lrs=\"{lrs}\" data-line=\"{line}\" data-cc=\"{cc}\" data-nd=\"{nd}\" \
                 data-tier=\"{tier}\" data-activity=\"{activity}\" data-churn=\"{churn}\" \
                 data-touches=\"{touches}\" data-fanin=\"{fanin}\" \
                 data-recency=\"{recency}\">\n\
                 <td class=\"monospace\">{file_display}</td>\n\
                 <td>{function_display}</td>\n\
                 <td>{line}</td>\n\
                 <td>{lrs:.2}</td>\n\
                 <td><span class=\"band-{band}\">{band}</span></td>\n\
                 <td>{cc}</td>\n\
                 <td>{nd}</td>\n\
                 <td>{fo}</td>\n\
                 <td>{ns}</td>\n\
                 {activity_cell}{churn_cell}{touches_cell}{recency_cell}{fanin_cell}\
                 </tr>",
                file = html_escape(&f.file),
                file_display = source_link(&f.file, f.line, &compact_source_label(&f.file)),
                function = html_escape(function_name),
                function_display = html_escape(function_name),
                tier = html_escape(tier_str),
                activity = f
                    .activity_risk
                    .map(|ar| format!("{:.4}", ar))
                    .unwrap_or_default(),
                churn = churn_val.map(|c| c.to_string()).unwrap_or_default(),
                touches = f.touch_count.map(|t| t.to_string()).unwrap_or_default(),
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
                band = f.band.as_str(),
                cc = f.metrics.cc,
                nd = f.metrics.nd,
                fo = f.metrics.fo,
                ns = f.metrics.ns,
                activity_cell = activity_cell,
                churn_cell = churn_cell,
                touches_cell = touches_cell,
                recency_cell = recency_cell,
                fanin_cell = fanin_cell,
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
        "<th class=\"sortable\" data-column=\"touches\" title=\"Number of commits touching this function in the last year\">Touches</th>"
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
    let gallery = render_function_risk_gallery(functions);

    format!(
        r#"<details class="section">
    <summary>Function Inventory (<span id="visible-count">{count}</span> of {count})<span class="section-summary-note">Full searchable table and function cards</span></summary>
    {gallery}

    <details class="raw-data-details">
        <summary>Show raw function table</summary>
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
            <label for="tier-filter">Activity</label>
            <select id="tier-filter">
                <option value="all">All</option>
                <option value="active">Active</option>
                <option value="quiet">Quiet</option>
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
                
            </tr>
        </thead>
        <tbody>
            {rows}
        </tbody>
    </table>
    </details>
</details>"#,
        count = functions.len(),
        gallery = gallery,
        rows = rows,
        activity_header = activity_header,
        churn_header = churn_header,
        touches_header = touches_header,
        recency_header = recency_header,
        fanin_header = fanin_header,
    )
}

fn by_activity_risk_desc(a: &&FunctionSnapshot, b: &&FunctionSnapshot) -> std::cmp::Ordering {
    b.activity_risk
        .unwrap_or(b.lrs)
        .partial_cmp(&a.activity_risk.unwrap_or(a.lrs))
        .unwrap_or(std::cmp::Ordering::Equal)
}

/// "Start here": the three active functions with the highest activity risk.
fn render_next_actions(functions: &[FunctionSnapshot]) -> String {
    let mut candidates: Vec<&FunctionSnapshot> = functions
        .iter()
        .filter(|f| f.tier.as_deref() == Some("active"))
        .collect();
    candidates.sort_by(by_activity_risk_desc);
    candidates.truncate(3);
    if candidates.is_empty() {
        return String::new();
    }

    let rows = candidates
        .into_iter()
        .enumerate()
        .map(|(idx, function)| render_next_action(idx + 1, function))
        .collect::<String>();

    format!(
        r#"<section class="section next-actions-section" id="next-actions">
    <h2>Start Here</h2>
    <div class="next-actions-subtitle">The three recently active functions with the highest activity risk. Activity risk rises when a file is edited and decays as it goes quiet; use the band to judge structure.</div>
    <div class="next-actions-list">{rows}</div>
</section>"#,
        rows = rows,
    )
}

fn render_next_action(rank: usize, function: &FunctionSnapshot) -> String {
    let function_name = function
        .function_id
        .split("::")
        .last()
        .unwrap_or(&function.function_id);
    let touches = function.touch_count.unwrap_or(0);
    let activity = function.activity_risk.unwrap_or(function.lrs);
    let last_change = function
        .days_since_last_change
        .map(|d| format!("{d}d ago"))
        .unwrap_or_else(|| "unknown".to_string());

    format!(
        r#"<div class="next-action next-action-active">
    <div class="next-action-rank">#{rank}</div>
    <div>
        <div class="next-action-title">{function}</div>
        <div class="next-action-meta monospace">{source}</div>
    </div>
    <div class="next-action-score">
        <span class="band-{band}">{band}</span>
        <span>{touches} touches</span>
        <span>{last_change}</span>
        <span>activity {activity:.2}</span>
    </div>
</div>"#,
        rank = rank,
        function = html_escape(function_name),
        source = source_link(
            &function.file,
            function.line,
            &format!("{}:{}", compact_source_label(&function.file), function.line)
        ),
        band = function.band.as_str(),
        touches = touches,
        last_change = last_change,
        activity = activity,
    )
}

/// Render triage panel: active/quiet counts + the top functions by activity risk.
fn render_triage_panel(functions: &[FunctionSnapshot]) -> String {
    if !functions.iter().any(|f| f.tier.is_some()) {
        return String::new();
    }

    let active = functions
        .iter()
        .filter(|f| f.tier.as_deref() == Some("active"))
        .count();
    let quiet = functions
        .iter()
        .filter(|f| f.tier.as_deref() == Some("quiet"))
        .count();

    let mut top_risks: Vec<&FunctionSnapshot> = functions.iter().collect();
    top_risks.sort_by(by_activity_risk_desc);
    top_risks.truncate(15);
    let count = top_risks.len();

    let rows: String = top_risks
        .into_iter()
        .map(|f| {
            let function_name = f.function_id.split("::").last().unwrap_or(&f.function_id);
            let risk_val = f.activity_risk.unwrap_or(f.lrs);
            let touches_value = f
                .touch_count
                .map(|t| t.to_string())
                .unwrap_or_else(|| "—".to_string());
            let last_change_value = f
                .days_since_last_change
                .map(|d| format!("{d}d"))
                .unwrap_or_else(|| "—".to_string());
            let fanin_value = f
                .callgraph
                .as_ref()
                .map(|cg| cg.fan_in.to_string())
                .unwrap_or_else(|| "—".to_string());
            let risk_width = (risk_val * 25.0).clamp(4.0, 100.0);
            let tier = f.tier.as_deref().unwrap_or("quiet");

            format!(
                r#"<div class="triage-risk-card {cls}">
    <div class="visual-card-title">{func}</div>
    <div class="visual-card-subtitle monospace">{source}</div>
    <div class="visual-bar"><div class="visual-bar-fill band-{band}" style="width:{risk_width:.0}%"></div></div>
    <div class="visual-metrics">
        <div class="visual-metric"><span>Band</span><strong class="band-{band}">{band}</strong></div>
        <div class="visual-metric"><span>Activity</span><strong>{risk:.2}</strong></div>
        <div class="visual-metric"><span>Touches</span><strong>{touches}</strong></div>
        <div class="visual-metric"><span>Fan-in</span><strong>{fanin}</strong></div>
    </div>
    <div class="visual-note">{tier} · last change {last_change}</div>
</div>"#,
                cls = if tier == "active" { "fire" } else { "" },
                source = source_link(&f.file, f.line, &format!("{}:{}", f.file, f.line)),
                func = html_escape(function_name),
                band = f.band.as_str(),
                risk = risk_val,
                risk_width = risk_width,
                tier = tier,
                touches = touches_value,
                last_change = last_change_value,
                fanin = fanin_value,
            )
        })
        .collect();

    let chips_html = format!(
        r#"<div class="tier-chips">
    <div class="tier-chip tier-active">
        <div class="chip-label">Active</div>
        <div class="chip-count">{active}</div>
        <div class="chip-desc">touched more than the repo median, or changed in the last 30 days</div>
    </div>
    <div class="tier-chip tier-quiet">
        <div class="chip-label">Quiet</div>
        <div class="chip-count">{quiet}</div>
        <div class="chip-desc">not recently active</div>
    </div>
</div>"#,
        active = active,
        quiet = quiet,
    );

    format!(
        r#"<details class="section triage-section" id="triage">
    <summary>Triage Details<span class="section-summary-note">Active and quiet counts, and the functions with the highest activity risk</span></summary>
    {chips}
    <h3 class="triage-subtitle">Top by Activity Risk ({count})</h3>
    <div class="triage-risk-grid">{rows}</div>
</details>"#,
        chips = chips_html,
        count = count,
        rows = rows,
    )
}

/// Render architecture/concentration sections.
fn render_aggregates(aggregates: &SnapshotAggregates) -> String {
    let mut sections = Vec::new();

    // 4a. File Risk Table (joined with FileAggregates for LRS metrics)
    if !aggregates.file_risk.is_empty() {
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
                let max_lrs = lrs.map(|l| l.max_lrs).unwrap_or(0.0);
                let high_plus = lrs.map(|l| l.high_plus_count).unwrap_or(0);
                let score_width = (f.file_risk_score * 10.0).clamp(4.0, 100.0);
                format!(
                    r#"<div class="visual-card">
    <div class="visual-card-title monospace">{file}</div>
    <div class="visual-card-subtitle">{fns} functions · {loc} LOC · {high_plus} high+</div>
    <div class="visual-bar"><div class="visual-bar-fill band-high" style="width:{score_width:.0}%"></div></div>
    <div class="visual-metrics">
        <div class="visual-metric"><span>Risk</span><strong>{score:.2}</strong></div>
        <div class="visual-metric"><span>Max LRS</span><strong>{max_lrs:.2}</strong></div>
        <div class="visual-metric"><span>Max CC</span><strong>{max_cc}</strong></div>
        <div class="visual-metric"><span>Critical</span><strong>{critical}</strong></div>
    </div>
</div>"#,
                    file = source_link(&f.file, 0, &f.file),
                    fns = f.function_count,
                    loc = f.loc,
                    max_cc = f.max_cc,
                    critical = f.critical_count,
                    high_plus = high_plus,
                    max_lrs = max_lrs,
                    score = f.file_risk_score,
                    score_width = score_width,
                )
            })
            .collect();

        sections.push(format!(
            r#"<details class="section" open>
    <summary>Risk Concentration: Files<span class="section-summary-note">Top files by composite risk</span></summary>
    <div class="visual-note">Use this to choose where to inspect first after the start-here actions.</div>
    <div class="visual-grid">{rows}</div>
</details>"#,
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
        let rows: String = qualifying
            .iter()
            .map(|p| {
                let width = (p.coupling_ratio * 100.0).clamp(4.0, 100.0);
                let band = if p.risk == "high" { "high" } else { "moderate" };
                format!(
                    r#"<div class="visual-card coupling-card">
    <div>
        <div class="visual-card-title monospace">{file_a}</div>
        <div class="visual-card-subtitle">source file</div>
    </div>
    <div class="coupling-link">{count}x<br>{ratio:.0}%</div>
    <div>
        <div class="visual-card-title monospace">{file_b}</div>
        <div class="visual-card-subtitle">{dep}</div>
    </div>
    <div class="visual-bar" style="grid-column:1 / -1"><div class="visual-bar-fill band-{band}" style="width:{width:.0}%"></div></div>
</div>"#,
                    file_a = source_link(&p.file_a, 0, &p.file_a),
                    file_b = source_link(&p.file_b, 0, &p.file_b),
                    count = p.co_change_count,
                    ratio = p.coupling_ratio * 100.0,
                    dep = if p.has_static_dep { "static dependency" } else { "implicit coupling" },
                    band = band,
                    width = width,
                )
            })
            .collect();

        sections.push(format!(
            r#"<details class="section">
    <summary>Risk Concentration: Co-change<span class="section-summary-note">Files that repeatedly change together</span></summary>
    <div class="visual-note">Bar length reflects coupling ratio.</div>
    <div class="visual-grid">{rows}</div>
</details>"#,
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

    let pr_summary_card = delta
        .aggregates
        .as_ref()
        .map(|a| &a.pr_summary)
        .map(|s| {
            format!(
                r#"
    <div class="summary-card">
        <h3>Changed Lines in Touched Functions</h3>
        <div class="value">{fn_changed_lines}</div>
        <p style="font-size:0.8em;margin-top:4px;">Strongest tested predictor of defect risk
        in our evaluation.</p>
    </div>
    <div class="summary-card">
        <h3>Highest Band</h3>
        <div class="value band-{band_class}">{band}</div>
    </div>"#,
                fn_changed_lines = s.fn_changed_lines,
                band = s.band.as_str(),
                band_class = s.band.as_str(),
            )
        })
        .unwrap_or_default();

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
    </div>{pr_summary_card}
</div>"#,
        new = new_count,
        modified = modified_count,
        deleted = deleted_count,
        regressions = regressions,
        pr_summary_card = pr_summary_card,
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
                    r#"<div class="visual-card">
    <div class="visual-card-title monospace">{function}</div>
    <div class="visual-card-subtitle">{policy}</div>
    <div class="visual-note">{message}</div>
</div>"#,
                    function = html_escape(function_id),
                    policy = result.id.as_str(),
                    message = html_escape(&result.message),
                )
            })
            .collect();

        sections.push(format!(
            r#"<div class="policy-failures">
    <h3>Blocking Failures ({count})</h3>
    <div class="visual-grid">{rows}</div>
</div>"#,
            count = policy.failed.len(),
            rows = rows,
        ));
    }

    let warnings: Vec<_> = policy.warnings.iter().collect();
    if !warnings.is_empty() {
        sections.push(render_warning_group("Warnings", &warnings, delta));
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

            let width = after_lrs.parse::<f64>().unwrap_or(0.0).mul_add(8.0, 0.0).clamp(4.0, 100.0);
            format!(
                r#"<div class="visual-card">
    <div class="visual-card-title monospace">{function}</div>
    <div class="visual-bar"><div class="visual-bar-fill band-moderate" style="width:{width:.0}%"></div></div>
    <div class="visual-metrics">
        <div class="visual-metric"><span>LRS</span><strong>{lrs}</strong></div>
    </div>
    <div class="visual-note">{message}</div>
</div>"#,
                function = html_escape(function_id),
                lrs = after_lrs,
                width = width,
                message = html_escape(&result.message),
            )
        })
        .collect();

    format!(
        r#"<div class="policy-warnings">
    <h3>{title} ({count})</h3>
    <div class="visual-grid">{rows}</div>
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
            let (file_name, function_name) = entry
                .function_id
                .split_once("::")
                .unwrap_or(("", &entry.function_id));
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

            let after_lrs_num = entry.after.as_ref().map(|a| a.lrs).unwrap_or(0.0);
            let width = (after_lrs_num * 8.0).clamp(4.0, 100.0);
            format!(
                r#"<div class="visual-card {status_class}" data-status="{status}" data-lrs="{after_lrs_val}">
    <div class="visual-card-title monospace">{function}</div>
    <div class="visual-card-subtitle monospace">{source}</div>
    <div class="visual-bar"><div class="visual-bar-fill band-{after_band}" style="width:{width:.0}%"></div></div>
    <div class="visual-metrics">
        <div class="visual-metric"><span>Before</span><strong>{before_lrs}</strong></div>
        <div class="visual-metric"><span>After</span><strong>{after_lrs}</strong></div>
        <div class="visual-metric"><span>Delta</span><strong>{delta_lrs}</strong></div>
        <div class="visual-metric"><span>Status</span><strong>{status_display}</strong></div>
    </div>
    <div class="visual-note"><span class="band-{before_band}">{before_band}</span> {transition} <span class="band-{after_band}">{after_band}</span></div>
</div>"#,
                status_class = status_class,
                status = status_lowercase,
                source = source_link(file_name, 0, file_name),
                function = html_escape(function_name),
                before_lrs = before_lrs,
                after_lrs = after_lrs,
                after_lrs_val = after_lrs_num,
                before_band = before_band,
                after_band = after_band,
                delta_lrs = delta_lrs,
                transition = transition,
                status_display = status_debug,
                width = width,
            )
        })
        .collect();

    format!(
        r#"<section class="section">
    <h2>Function Changes ({count})</h2>
    <div class="visual-note">Function changes rendered as risk bars. Bar length reflects after-change LRS.</div>
    <div id="delta-table" class="visual-grid">{rows}</div>
</section>"#,
        count = deltas.len(),
        rows = rows,
    )
}

/// Render footer
fn render_footer() -> String {
    r#"<footer>
    <p>
        Generated by <a href="https://github.com/Stephen-Collins-tech/hotspots" target="_blank" rel="noopener">hotspots</a>
        — activity-weighted code risk analysis. ·
        <a href="https://docs.hotspots.dev/reference/metrics" target="_blank" rel="noopener">How metrics are calculated →</a>
    </p>
</footer>"#
        .to_string()
}

/// Render an optional banner linking back to a written analysis post.
/// Returns empty string when `url` is `None`.
fn render_source_banner(url: Option<&str>) -> String {
    match url {
        Some(u) => format!(
            r#"<div class="source-banner">
    This report is accompanied by a written analysis with function-specific recommendations:
    <a href="{url}" target="_blank" rel="noopener">Read the full analysis →</a>
</div>"#,
            url = html_escape(u),
        ),
        None => String::new(),
    }
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

fn source_link(file: &str, line: u32, label: &str) -> String {
    let href = source_href(file, line);
    let display_label = compact_source_label(label);
    format!(
        r#"<a class="source-link" href="{href}">{label}</a>"#,
        href = html_escape(&href),
        label = html_escape(&display_label),
    )
}

fn compact_source_label(label: &str) -> String {
    let normalized = label.replace('\\', "/");
    if let Some((path, suffix)) = normalized.rsplit_once(':') {
        if suffix.chars().all(|ch| ch.is_ascii_digit()) {
            return format!("{}:{suffix}", compact_source_label(path));
        }
    }
    if let Some(idx) = normalized.find("/hotspots/") {
        return normalized[idx + "/hotspots/".len()..].to_string();
    }
    if normalized.starts_with('/') {
        let parts: Vec<&str> = normalized
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        let keep = parts.len().saturating_sub(4);
        return parts[keep..].join("/");
    }
    normalized
}

fn source_href(file: &str, line: u32) -> String {
    let normalized = file.replace('\\', "/");
    let base = if normalized.starts_with('/') {
        format!("file://{}", normalized)
    } else {
        normalized
    };
    let encoded = base
        .chars()
        .map(|ch| match ch {
            ' ' => "%20".to_string(),
            '"' => "%22".to_string(),
            '<' => "%3C".to_string(),
            '>' => "%3E".to_string(),
            '#' => "%23".to_string(),
            '?' => "%3F".to_string(),
            _ => ch.to_string(),
        })
        .collect::<String>();
    if line > 0 {
        format!("{encoded}#L{line}")
    } else {
        encoded
    }
}
