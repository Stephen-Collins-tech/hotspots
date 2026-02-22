# Integration Tests

End-to-end tests for the Hotspots CLI. These tests:
- Build a temporary git repo with TypeScript files
- Run `hotspots` in snapshot/delta/trends modes
- Validate JSON outputs parse and contain expected sections

Run locally:
- `make build && pytest -q integration`
- Fallback: `python3 integration/legacy/test_comprehensive.py` (legacy script)

CI:
- GitHub Actions installs `pytest` and runs `pytest -q integration` via `make test-comprehensive`.
