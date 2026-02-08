//! HTML report generation
//!
//! Generates self-contained HTML reports with embedded CSS and JavaScript.
//! Reports are interactive (sorting, filtering) and work offline.

use crate::snapshot::{CommitInfo, FunctionSnapshot, Snapshot};
use crate::aggregates::SnapshotAggregates;
use crate::delta::{Delta, FunctionDeltaEntry, FunctionStatus};
use crate::policy::{PolicyResults, PolicyId};

/// Render a snapshot as an HTML report
pub fn render_html_snapshot(snapshot: &Snapshot) -> String {
    let aggregates = snapshot.aggregates.as_ref();

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
        functions_table = render_functions_table(&snapshot.functions),
        aggregates_section = aggregates.map(render_aggregates).unwrap_or_default(),
        footer = render_footer(),
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
        policy_section = delta.policy.as_ref().map(|p| render_policy_section(p, delta)).unwrap_or_default(),
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
}
"#
}

/// Inline JavaScript for interactivity
fn inline_javascript() -> &'static str {
    r#"
// Table sorting and filtering
(function() {
    let sortColumn = 'lrs';
    let sortDirection = 'desc';

    function sortTable(column) {
        const table = document.querySelector('#functions-table tbody');
        const rows = Array.from(table.querySelectorAll('tr'));

        // Toggle direction if same column
        if (sortColumn === column) {
            sortDirection = sortDirection === 'asc' ? 'desc' : 'asc';
        } else {
            sortColumn = column;
            sortDirection = 'desc';  // Default descending for new column
        }

        // Update header indicators
        document.querySelectorAll('th.sortable').forEach(th => {
            th.classList.remove('asc', 'desc');
        });
        const activeHeader = document.querySelector(`th[data-column="${column}"]`);
        if (activeHeader) {
            activeHeader.classList.add(sortDirection);
        }

        // Sort rows
        rows.sort((a, b) => {
            let aVal = a.dataset[column] || '';
            let bVal = b.dataset[column] || '';

            // Try numeric comparison
            const aNum = parseFloat(aVal);
            const bNum = parseFloat(bVal);
            if (!isNaN(aNum) && !isNaN(bNum)) {
                return sortDirection === 'asc' ? aNum - bNum : bNum - aNum;
            }

            // String comparison
            if (sortDirection === 'asc') {
                return aVal.localeCompare(bVal);
            } else {
                return bVal.localeCompare(aVal);
            }
        });

        // Re-append rows
        rows.forEach(row => table.appendChild(row));
    }

    function filterTable() {
        const bandFilter = document.getElementById('band-filter').value;
        const searchFilter = document.getElementById('search-filter').value.toLowerCase();

        const rows = document.querySelectorAll('#functions-table tbody tr');

        rows.forEach(row => {
            const band = row.dataset.band;
            const func = row.dataset.function.toLowerCase();
            const file = row.dataset.file.toLowerCase();

            const bandMatch = bandFilter === 'all' || band === bandFilter;
            const searchMatch = !searchFilter ||
                func.includes(searchFilter) ||
                file.includes(searchFilter);

            row.style.display = (bandMatch && searchMatch) ? '' : 'none';
        });

        // Update count
        const visibleCount = Array.from(rows).filter(r => r.style.display !== 'none').length;
        const countEl = document.getElementById('visible-count');
        if (countEl) {
            countEl.textContent = visibleCount;
        }
    }

    // Initialize on load
    document.addEventListener('DOMContentLoaded', function() {
        // Attach sort handlers
        document.querySelectorAll('th.sortable').forEach(th => {
            th.addEventListener('click', function() {
                sortTable(this.dataset.column);
            });
        });

        // Attach filter handlers
        const bandFilter = document.getElementById('band-filter');
        const searchFilter = document.getElementById('search-filter');

        if (bandFilter) {
            bandFilter.addEventListener('change', filterTable);
        }

        if (searchFilter) {
            searchFilter.addEventListener('input', filterTable);
        }

        // Initial sort by LRS descending
        sortTable('lrs');
    });
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
    let critical_count = snapshot.functions.iter().filter(|f| f.band == "critical").count();
    let high_count = snapshot.functions.iter().filter(|f| f.band == "high").count();
    let avg_lrs = if total_functions > 0 {
        snapshot.functions.iter().map(|f| f.lrs).sum::<f64>() / total_functions as f64
    } else {
        0.0
    };

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
    </div>
</div>"#,
        total = total_functions,
        critical = critical_count,
        high = high_count,
        avg = avg_lrs,
    )
}

/// Render functions table
fn render_functions_table(functions: &[FunctionSnapshot]) -> String {
    let rows: String = functions
        .iter()
        .map(|f| {
            let function_name = f.function_id.split("::").last().unwrap_or(&f.function_id);

            format!(
                r#"<tr data-file="{file}" data-function="{function}" data-band="{band}" data-lrs="{lrs}" data-line="{line}" data-cc="{cc}" data-nd="{nd}">
    <td class="monospace">{file_display}</td>
    <td>{function_display}</td>
    <td>{line}</td>
    <td>{lrs:.2}</td>
    <td><span class="band-{band}">{band}</span></td>
    <td>{cc}</td>
    <td>{nd}</td>
    <td>{fo}</td>
    <td>{ns}</td>
</tr>"#,
                file = html_escape(&f.file),
                file_display = html_escape(&f.file),
                function = html_escape(function_name),
                function_display = html_escape(function_name),
                line = f.line,
                lrs = f.lrs,
                band = &f.band,
                cc = f.metrics.cc,
                nd = f.metrics.nd,
                fo = f.metrics.fo,
                ns = f.metrics.ns,
            )
        })
        .collect();

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
            <label for="search-filter">Search</label>
            <input type="text" id="search-filter" placeholder="Function or file name...">
        </div>
    </div>

    <table id="functions-table">
        <thead>
            <tr>
                <th class="sortable" data-column="file">File</th>
                <th class="sortable" data-column="function">Function</th>
                <th class="sortable" data-column="line">Line</th>
                <th class="sortable" data-column="lrs">LRS</th>
                <th class="sortable" data-column="band">Band</th>
                <th class="sortable" data-column="cc">CC</th>
                <th class="sortable" data-column="nd">ND</th>
                <th>FO</th>
                <th>NS</th>
            </tr>
        </thead>
        <tbody>
            {rows}
        </tbody>
    </table>
</section>"#,
        count = functions.len(),
        rows = rows,
    )
}

/// Render aggregates section
fn render_aggregates(aggregates: &SnapshotAggregates) -> String {
    if aggregates.files.is_empty() {
        return String::new();
    }

    let mut files = aggregates.files.clone();
    files.sort_by(|a, b| b.sum_lrs.partial_cmp(&a.sum_lrs).unwrap());

    let rows: String = files
        .iter()
        .take(20)  // Top 20 files
        .map(|f| {
            format!(
                r#"<tr>
    <td class="monospace">{file}</td>
    <td>{sum:.2}</td>
    <td>{max:.2}</td>
    <td>{high_plus}</td>
</tr>"#,
                file = html_escape(&f.file),
                sum = f.sum_lrs,
                max = f.max_lrs,
                high_plus = f.high_plus_count,
            )
        })
        .collect();

    format!(
        r#"<section class="section">
    <h2>File Aggregates (Top 20 by Total LRS)</h2>
    <table>
        <thead>
            <tr>
                <th>File</th>
                <th>Total LRS</th>
                <th>Max LRS</th>
                <th>High+ Functions</th>
            </tr>
        </thead>
        <tbody>
            {rows}
        </tbody>
    </table>
</section>"#,
        rows = rows,
    )
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
        parent = if commit.parent.is_empty() { "none" } else { &commit.parent[..8] },
    )
}

/// Render delta summary
fn render_delta_summary(delta: &Delta) -> String {
    let new_count = delta.deltas.iter().filter(|d| d.status == FunctionStatus::New).count();
    let modified_count = delta.deltas.iter().filter(|d| d.status == FunctionStatus::Modified).count();
    let deleted_count = delta.deltas.iter().filter(|d| d.status == FunctionStatus::Deleted).count();
    let regressions = delta.deltas.iter()
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
        let rows: String = policy.failed.iter().map(|result| {
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
        }).collect();

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
    let watch_warnings: Vec<_> = policy.warnings.iter().filter(|w| w.id == PolicyId::WatchThreshold).collect();
    let attention_warnings: Vec<_> = policy.warnings.iter().filter(|w| w.id == PolicyId::AttentionThreshold).collect();
    let rapid_growth_warnings: Vec<_> = policy.warnings.iter().filter(|w| w.id == PolicyId::RapidGrowth).collect();

    if !watch_warnings.is_empty() {
        sections.push(render_warning_group("Watch Level", &watch_warnings, delta));
    }
    if !attention_warnings.is_empty() {
        sections.push(render_warning_group("Attention Level", &attention_warnings, delta));
    }
    if !rapid_growth_warnings.is_empty() {
        sections.push(render_warning_group("Rapid Growth", &rapid_growth_warnings, delta));
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

fn render_warning_group(title: &str, warnings: &[&crate::policy::PolicyResult], delta: &Delta) -> String {
    let rows: String = warnings.iter().map(|result| {
        let function_id = result.function_id.as_deref().unwrap_or("N/A");
        let entry = delta.deltas.iter().find(|e| e.function_id == function_id);
        let after_lrs = entry.and_then(|e| e.after.as_ref()).map(|a| format!("{:.2}", a.lrs)).unwrap_or_else(|| "N/A".to_string());

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
    }).collect();

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
            let function_name = entry.function_id.split("::").last().unwrap_or(&entry.function_id);
            let before_lrs = entry.before.as_ref().map(|b| format!("{:.2}", b.lrs)).unwrap_or_else(|| "-".to_string());
            let after_lrs = entry.after.as_ref().map(|a| format!("{:.2}", a.lrs)).unwrap_or_else(|| "-".to_string());
            let before_band = entry.before.as_ref().map(|b| b.band.as_str()).unwrap_or("-");
            let after_band = entry.after.as_ref().map(|a| a.band.as_str()).unwrap_or("-");
            let delta_lrs = entry.delta.as_ref().map(|d| format!("{:+.2}", d.lrs)).unwrap_or_else(|| "-".to_string());

            let transition = match (entry.before.as_ref().map(|b| &b.band), entry.after.as_ref().map(|a| &a.band)) {
                (Some(b), Some(a)) if b != a => {
                    if a > b { "↑" } else { "↓" }
                }
                (Some(_), Some(_)) => "→",
                _ => "-",
            };

            let status_class = match entry.status {
                FunctionStatus::New => "status-new",
                FunctionStatus::Deleted => "status-deleted",
                FunctionStatus::Modified => if entry.delta.as_ref().map(|d| d.lrs > 0.0).unwrap_or(false) { "status-regression" } else { "" },
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
</footer>"#.to_string()
}

/// Format Unix timestamp as human-readable string
fn format_timestamp(timestamp: i64) -> String {
    // Simple formatting - just show as ISO 8601-ish
    use std::time::UNIX_EPOCH;
    let duration = std::time::Duration::from_secs(timestamp as u64);
    let datetime = UNIX_EPOCH + duration;

    // Format as YYYY-MM-DD HH:MM:SS (deterministic, no timezone)
    format!("{:?}", datetime)  // Temporary - will improve later
}

/// Escape HTML special characters
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
