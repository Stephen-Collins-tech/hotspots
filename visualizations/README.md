# Faultline Visualizations

Interactive charts showing code risk metrics over time using Vega-Lite.

Open `index.html` in a browser (or serve this folder with a simple HTTP server).

## Files

- `data.json` - input data (auto-generated from snapshots)
- `specs/` - Vega-Lite chart specifications
  - `lrs_timeline.vl.json` - LRS line chart with band thresholds
  - `band_timeline.vl.json` - band step timeline
  - `delta_breakdown.vl.json` - per-commit delta breakdown (dropdown)
  - `repo_distribution.vl.json` - LRS histogram with hotspot marker
  - `risk_concentration.vl.json` - stacked area chart showing Top K functions + Other over time
- `index.html` - viewer that embeds all charts

## Generating Data

To generate `data.json` from Faultline snapshots:

```bash
# From repository root
cargo run --example export_visualization
```

Or with custom options:

```bash
# Specify different repo path
cargo run --example export_visualization -- --repo ../my-repo

# Specify target function explicitly
cargo run --example export_visualization -- --target-function "src/api.ts::handler"

# Specify output directory
cargo run --example export_visualization -- --output-dir ./custom-output
```

This script:
- Reads snapshots from the specified repo's `.faultline/snapshots/` directory
- **Auto-detects target function** (highest LRS in latest snapshot) if not specified
- Extracts hotspot function timeline data
- Extracts deltas between commits
- Extracts repo distribution from latest snapshot
- Extracts risk concentration data (Top K functions + Other per commit)
- Updates `data.json` (creates from scratch if it doesn't exist)

**Prerequisites:**
- Snapshots must exist in the repo (created via `faultline analyze --mode snapshot`)
- If `data.json` doesn't exist, the script will:
  - Auto-detect the function with highest LRS as the target
  - Extract repo name from the directory name
  - Generate all data from scratch

## Local preview

From this directory:

```bash
python3 -m http.server 8000
```

Then open:

http://localhost:8000/index.html

## Charts

### Risk Concentration Chart

The risk concentration chart shows how LRS is distributed across functions over time:
- **Top K functions** (default: 5, configurable via `--top-k`) are shown as individual stack layers
- **Other** bucket contains the sum of all remaining functions
- **Y-axis** shows cumulative LRS (stacked sum)
- **X-axis** shows commit order (deterministic: timestamp ascending, SHA tie-break)

This visualization helps identify:
- Whether risk is concentrated in a few hotspots or spread across many functions
- How refactoring affects risk distribution (concentration changes)
- Whether "Other" bucket is growing or shrinking over time
