#!/usr/bin/env bash
# Reproduces the hotspots OSV/bug-correlation benchmark (see benchmarks/README.md).
#
# Usage: ./benchmarks/run.sh /path/to/hotspots
#
# Clones each repo in corpus.json (bare, or reuses an existing clone), checks
# out the pinned feature SHA (or resolves "last commit before label_after" and
# pins it back into corpus.json on first run), runs `hotspots analyze`, labels
# bug-fix commits in the label window, scores rho/P@10 per repo, and writes
# benchmarks/versions/vX.Y.Z.json + appends a RESULTS.md block.
set -euo pipefail

HOTSPOTS_BIN="${1:?Usage: $0 /path/to/hotspots}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKDIR="${HOTSPOTS_BENCH_DIR:-/tmp/hotspots-bench}"
CLONES="$WORKDIR/clones"
mkdir -p "$CLONES"

HOTSPOTS_VERSION="$(cd "$SCRIPT_DIR/.." && grep -m1 '^version' Cargo.toml | sed -E 's/version = "(.*)"/\1/')"
RUN_DATE="$(date +%Y-%m-%d)"

python3 "$SCRIPT_DIR/_run_corpus.py" \
  --corpus "$SCRIPT_DIR/corpus.json" \
  --clones "$CLONES" \
  --workdir "$WORKDIR" \
  --hotspots "$HOTSPOTS_BIN" \
  --hotspots-version "$HOTSPOTS_VERSION" \
  --run-date "$RUN_DATE" \
  --label-py "$SCRIPT_DIR/label.py" \
  --score-py "$SCRIPT_DIR/score.py" \
  --versions-dir "$SCRIPT_DIR/versions" \
  --results-md "$SCRIPT_DIR/RESULTS.md"
