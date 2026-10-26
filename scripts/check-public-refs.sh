#!/usr/bin/env bash
# Fails if tracked files reference private research material or local machine paths.
# This repo is public and must stand alone (see CLAUDE.md, "Public repo: no private references").
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

pattern='(^|[^A-Za-z0-9_])F[0-9]{2,3}([^A-Za-z0-9_]|$)|META-[0-9]+|promotion-brief|/Users/stephencollins|\.\./hotspots-[a-z]+'
allow='\.\./hotspots-(core|cli|content)([^a-z]|$)'

hits=$(git grep -n -I -E "$pattern" -- . \
  ':!*.lock' ':!action/dist' ':!action/package-lock.json' ':!scripts/check-public-refs.sh' \
  ':!CHANGELOG.md' ':!CLAUDE.md' \
  | grep -v -E "$allow" || true)

if [ -n "$hits" ]; then
  echo "Private or local references found:" >&2
  echo "$hits" >&2
  exit 1
fi
