# Faultline Synthetic - Vega Visualizations

Open `index.html` in a browser (or serve this folder with a simple HTTP server).

## Files

- `data.json` - input data (auto-generated from snapshots)
- `lrs_timeline.vl.json` - LRS line chart with band thresholds
- `band_timeline.vl.json` - band step timeline
- `delta_breakdown.vl.json` - per-commit delta breakdown (dropdown)
- `repo_distribution.vl.json` - LRS histogram with hotspot marker
- `index.html` - viewer that embeds all charts

## Updating Data

To update `data.json` from Faultline snapshots:

```bash
cd analysis
cargo run --bin update-report
```

Or with custom options:

```bash
# Specify different repo path
cargo run --bin update-report -- --repo ../my-repo

# Specify target function explicitly
cargo run --bin update-report -- --target-function "src/api.ts::handler"

# Specify output directory
cargo run --bin update-report -- --output-dir ./custom-output
```

This script:
- Reads snapshots from the specified repo's `.faultline/snapshots/` directory
- **Auto-detects target function** (highest LRS in latest snapshot) if not specified
- Extracts hotspot function timeline data
- Extracts deltas between commits
- Extracts repo distribution from latest snapshot
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
