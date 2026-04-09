#!/bin/sh
# Dispatch table for benchmark operations.
# Called as: benchmark-entrypoint <op>
# Using an entrypoint avoids fly CLI argument-parsing ambiguity when the
# command contains flags that look like fly flags (e.g. git -c pack.threads=2).
set -e

REPO=/repo
REPO_URL="https://github.com/expo/expo.git"

# BENCH_OP env var takes priority over the CMD arg so that
# `fly machine run --env BENCH_OP=probe` works even when Dockerfile CMD
# defaults to "analyze" (which would otherwise always shadow the env var).
case "${BENCH_OP:-${1:-analyze}}" in
  analyze)
    cd "$REPO"
    JOBS_ARG=""
    if [ -n "${BENCH_JOBS:-}" ]; then
        JOBS_ARG="--jobs ${BENCH_JOBS}"
    fi
    CALLGRAPH_SKIP_ARG=""
    if [ -n "${BENCH_CALLGRAPH_SKIP_ABOVE:-}" ]; then
        CALLGRAPH_SKIP_ARG="--callgraph-skip-above ${BENCH_CALLGRAPH_SKIP_ABOVE}"
    fi
    # Per-function touches are disabled by default in the benchmark because the
    # cold-start spawns one git log -L subprocess per function (~51k for expo/expo),
    # completely dominating CPU. Set BENCH_PER_FUNCTION_TOUCHES=1 to enable.
    TOUCH_ARG="--no-per-function-touches"
    if [ -n "${BENCH_PER_FUNCTION_TOUCHES:-}" ]; then
        TOUCH_ARG="--per-function-touches"
    fi
    exec /usr/local/bin/hotspots analyze . \
        --mode snapshot --format json --no-persist $JOBS_ARG $CALLGRAPH_SKIP_ARG $TOUCH_ARG
    ;;

  probe)
    # Prints exactly one of: ok | partial | missing
    if [ ! -d "$REPO/.git" ]; then
        echo missing
    elif ! git -C "$REPO" rev-parse --verify HEAD >/dev/null 2>&1; then
        echo partial
    else
        echo ok
    fi
    ;;

  clone)
    exec git -c pack.threads=2 clone --progress "$REPO_URL" "$REPO"
    ;;

  clear)
    find "$REPO" -mindepth 1 -delete 2>/dev/null || true
    echo "Cleared $REPO"
    ;;

  stats)
    commits=$(git -C "$REPO" rev-list --count HEAD 2>/dev/null || echo unknown)
    files=$(git -C "$REPO" ls-files 2>/dev/null | wc -l | tr -d ' ')
    printf 'BENCH_COMMITS=%s\n' "$commits"
    printf 'BENCH_FILES=%s\n' "$files"
    ;;

  *)
    # Pass-through for arbitrary shell commands
    exec "$@"
    ;;
esac
