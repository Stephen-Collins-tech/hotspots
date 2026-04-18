#!/usr/bin/env bash
# Watch RSS memory of a hotspots run and print a sample every INTERVAL seconds.
#
# Usage:
#   ./scripts/rss-watch.sh [--interval N] [--csv FILE] -- <hotspots args...>
#
# Examples:
#   ./scripts/rss-watch.sh -- analyze /path/to/repo --mode snapshot
#   ./scripts/rss-watch.sh --interval 0.5 --csv /tmp/rss.csv -- analyze . --mode snapshot
#   ./scripts/rss-watch.sh --csv /tmp/rss.csv -- analyze /path/to/expo/expo --mode snapshot \
#       --callgraph-skip-above 500000
#
# Output columns (stdout + optional CSV):
#   elapsed_s  rss_mb  peak_rss_mb
#
# On macOS, RSS comes from `ps -o rss=` (kilobytes).
# On Linux,  RSS comes from /proc/<pid>/status VmRSS (kilobytes).

set -euo pipefail

INTERVAL=1
CSV_FILE=""

# Parse options until we see --
while [[ $# -gt 0 ]]; do
    case "$1" in
        --interval)
            INTERVAL="$2"; shift 2 ;;
        --csv)
            CSV_FILE="$2"; shift 2 ;;
        --)
            shift; break ;;
        *)
            echo "unknown option: $1" >&2
            echo "usage: $0 [--interval N] [--csv FILE] -- <hotspots args...>" >&2
            exit 1 ;;
    esac
done

if [[ $# -eq 0 ]]; then
    echo "error: no hotspots arguments given after --" >&2
    echo "usage: $0 [--interval N] [--csv FILE] -- <hotspots args...>" >&2
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARY="$REPO_ROOT/target/release/hotspots"

if [[ ! -x "$BINARY" ]]; then
    echo "building hotspots release binary..."
    (cd "$REPO_ROOT" && cargo build --release --bin hotspots 2>&1)
fi

get_rss_kb() {
    local pid="$1"
    if [[ "$(uname)" == "Darwin" ]]; then
        ps -o rss= -p "$pid" 2>/dev/null | tr -d ' ' || echo 0
    else
        # Linux: /proc/<pid>/status VmRSS line
        awk '/^VmRSS:/{print $2; exit}' /proc/"$pid"/status 2>/dev/null || echo 0
    fi
}

# Write CSV header if requested
if [[ -n "$CSV_FILE" ]]; then
    echo "elapsed_s,rss_mb,peak_rss_mb" > "$CSV_FILE"
fi

printf "%-12s %-12s %-12s\n" "elapsed_s" "rss_mb" "peak_rss_mb"
printf "%-12s %-12s %-12s\n" "---------" "------" "-----------"

# Launch the binary in the background
"$BINARY" "$@" &
PID=$!
ms_now() { python3 -c "import time; print(int(time.time()*1000))"; }

START=$(ms_now)
PEAK_KB=0
EXIT_CODE=0

while kill -0 "$PID" 2>/dev/null; do
    RSS_KB=$(get_rss_kb "$PID")
    NOW=$(ms_now)
    ELAPSED_MS=$(( NOW - START ))
    ELAPSED_S=$(awk "BEGIN{printf \"%.1f\", $ELAPSED_MS/1000}")
    RSS_MB=$(awk "BEGIN{printf \"%.1f\", $RSS_KB/1024}")

    if (( RSS_KB > PEAK_KB )); then
        PEAK_KB=$RSS_KB
    fi
    PEAK_MB=$(awk "BEGIN{printf \"%.1f\", $PEAK_KB/1024}")

    printf "%-12s %-12s %-12s\n" "$ELAPSED_S" "$RSS_MB" "$PEAK_MB"

    if [[ -n "$CSV_FILE" ]]; then
        echo "$ELAPSED_S,$RSS_MB,$PEAK_MB" >> "$CSV_FILE"
    fi

    sleep "$INTERVAL"
done

wait "$PID" || EXIT_CODE=$?

# Final sample after exit
NOW=$(ms_now)
ELAPSED_MS=$(( NOW - START ))
ELAPSED_S=$(awk "BEGIN{printf \"%.1f\", $ELAPSED_MS/1000}")
PEAK_MB=$(awk "BEGIN{printf \"%.1f\", $PEAK_KB/1024}")

echo ""
echo "--- done in ${ELAPSED_S}s | peak RSS: ${PEAK_MB} MB | exit: $EXIT_CODE ---"

if [[ -n "$CSV_FILE" ]]; then
    echo "CSV written to: $CSV_FILE"
fi

exit $EXIT_CODE
