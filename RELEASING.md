# Release Process

Releases are automated via [release-please](https://github.com/googleapis/release-please).
The only manual step is merging the Release PR.

---

## How it works

```
Developer merges PR to main
        ↓
release-please reads commit messages
        ↓
release-please opens/updates a "Release PR"
(bumps Cargo.toml version + writes CHANGELOG.md)
        ↓
Maintainer merges the Release PR
        ↓
release-please pushes tag vX.Y.Z
        ↓
release.yml triggers: builds Linux / macOS / Windows binaries
        ↓
GitHub Release published with binaries attached
```

---

## Commit types → version bump

release-please reads conventional commit prefixes to decide the bump:

| Prefix | Bump | Example |
|---|---|---|
| `feat:` | **MINOR** | new CLI flag, new output field |
| `fix:`, `perf:` | **PATCH** | bug fix, performance improvement |
| `feat!:` / `fix!:` / `BREAKING CHANGE:` footer | **MAJOR** | removed flag, changed exit codes, renamed JSON field |
| `chore:`, `docs:`, `ci:`, `refactor:`, `test:` | none | no release created |

If only non-releasing commits land on main, no Release PR is opened until a `feat:` or `fix:` arrives.

---

## Cutting a release

1. Merge your feature/fix PRs to `main` as normal.
2. release-please opens (or updates) a PR titled **"chore(main): release vX.Y.Z"** automatically.
3. Review the PR — it contains the updated `CHANGELOG.md` and bumped `Cargo.toml` version.
4. Merge it. The tag is pushed, binaries are built, and the GitHub Release is published.

That's it.

---

## Pre-releases / RCs

To cut a release candidate without waiting for the Release PR:

```bash
git tag v2.0.0-rc.1
git push origin v2.0.0-rc.1
```

The `release.yml` workflow will build binaries and create a pre-release on GitHub for any tag containing a hyphen. Do not merge the Release PR until the RC is promoted to stable.

---

## Emergency: manual release

If automation is broken and a release must go out immediately:

```bash
# 1. Edit CHANGELOG.md — move [Unreleased] to [X.Y.Z] - YYYY-MM-DD
# 2. Bump version in Cargo.toml [workspace.package]
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test

git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore: release vX.Y.Z"
git tag vX.Y.Z
git push origin main
git push origin vX.Y.Z
```

After a manual release, update `.release-please-manifest.json` to match the new version
so release-please stays in sync:

```json
{ ".": "X.Y.Z" }
```

---

## Configuration files

| File | Purpose |
|---|---|
| `.github/workflows/release-please.yml` | Runs release-please on every push to main |
| `release-please-config.json` | Tells release-please this is a Rust workspace at the repo root |
| `.release-please-manifest.json` | Tracks the last released version (updated automatically by release-please) |
| `.github/workflows/release.yml` | Builds binaries and publishes GitHub Release on any `v*` tag |
