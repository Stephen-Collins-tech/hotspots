# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.27.0] - 2026-07-02

### Bug Fixes
- Add --label-before to hotspots train to bound the label window


### Documentation
- Add benchmark release checklist; fix flaky pre-commit test skip
- Restructure RESULTS.md — latest block + history summary table

## [1.26.0] - 2026-06-28

### Bug Fixes
- Sliding-window ETA replaces cumulative average in train progress
- Clippy — remove unnecessary cast, use saturating_sub


### Documentation
- Add npm and pip install channels to README (#107)
- Compress 55 docs into 5 pages (#108)
- Add requirements specs for next release (REQ-001 through REQ-004)
- Correct REQ-002 — convention_bug_fix_count field does not yet exist on FunctionSnapshot
- Update train UX, v5 model features, and setup instructions


### Features
- REQ-002 — add convention_bug_fix_count as 10th ranker feature (model_version 5)
- Train progress — per-tree ETA, estimate+prompt, --yes/-y, --quiet/-q, elapsed

## [1.25.3] - 2026-06-19

### Bug Fixes
- Use subshells for npm publish to isolate working dirs
- Scope wrapper npm package to @stephencollinstech/hotspots (#106)

## [1.25.2] - 2026-06-19

### Bug Fixes
- Use ./ prefix for npm publish local paths (#105)

## [1.25.1] - 2026-06-19

### Features
- Add npm and PyPI distribution channels (#104)

## [1.25.0] - 2026-06-17

### Bug Fixes
- Python try/except/finally CFG when all branches return (#99)
- Name TS/TSX functions from their assigned variable (#102)


### Documentation
- Revamp core pages for clarity and add C language support (#92)
- Remove duplication, fix stale content, trim boilerplate (#93)
- Update README for C/C# languages, GitHub Action, SARIF, init hooks (#94)
- Add scoring methodology and changelog pages (#96)
- Document --screen flag and TS/TSX function naming (#103)


### Features
- Add hotspots init --ci with GitHub Actions workflow (#101)
- Add hotspots train --screen pre-flight repo screener (F08) (#91)

## [1.24.0] - 2026-06-07

### Documentation
- Document --skip-gate, color output, and installer update check (#90)


### Features
- Add version check and update prompt to installer (#74)
- Color-coded terminal output (red/yellow/green by band) (#82)
- Add suppression gate to detect activity ranker failure (#78)
- Hotspots train --eval with P@K output (#84)

## [1.23.3] - 2026-06-07

### Bug Fixes
- Lazy join_node in C if/else prevents orphan CFG nodes (#89)

## [1.23.2] - 2026-06-07

### Features
- Add C/C++ vendored dirs to default excludes (#88)

## [1.23.1] - 2026-06-06

### Features
- Add C language support (tree-sitter-c) (#86)

## [1.23.0] - 2026-06-06

### Features
- Directed coupling signal (F37/F38/F39) (#85)

## [1.22.0] - 2026-06-04

### Features
- Actionable band-grouped output with color (#83)

## [1.21.1] - 2026-06-02

### Documentation
- Add codebase guide (#77)


### Features
- Hotspots train — RandomForest ranker from fix-commit history (v3) (#79)

## [1.20.1] - 2026-05-24

### Features
- Compact default analyze output, grouped by file, top 10 (#75)

## [1.20.0] - 2026-05-24

### Bug Fixes
- Use relative path in csharp-switches golden file


### Features
- Add C# language support (.cs files) (#71)

## [1.18.0] - 2026-05-20

### Features
- Implement compact levels 1 and 2 with --dry-run (#69)

## [1.17.0] - 2026-05-20

### Features
- Add subsystem field to FunctionSnapshot for monorepo signal isolation
- Add model risk map mode

## [1.16.1] - 2026-05-01

### Bug Fixes
- Always merge default excludes with user config; bound --top N memory with heap (#66)

## [1.16.0] - 2026-05-01

### Features
- Make hybrid touch mode the default for all repo sizes (#65)

## [1.15.1] - 2026-04-24

### Bug Fixes
- Prevent OOM on large monorepos (expo/expo: 107 MB peak RSS) (#63)

## [1.15.0] - 2026-04-20

### Bug Fixes
- Reduce peak RSS via index-based CallGraph, enum types, and hybrid touch mode (#62)

## [1.14.0] - 2026-04-17

### Bug Fixes
- Resolve clippy lint errors from Rust 1.95 stable (#60)


### Features
- Warn on cold cache miss + per-item progress for touch metrics (#59)

## [1.13.0] - 2026-04-12

### Performance
- Large repo OOM — SQLite snapshot store, streaming JSON, call graph tuning (#58)

## [1.12.0] - 2026-03-31

### Bug Fixes
- Unique worktree paths and exit 2 on auto-analyze failure (#57)


### Features
- Implement --auto-analyze for hotspots diff (#56)

## [1.11.1] - 2026-03-27

### Features
- Add hotspots CI workflow and action diff support (#50)

## [1.11.0] - 2026-03-26

### Documentation
- Document SARIF format and hotspots init --hooks (#48)


### Features
- Add hotspots diff <base> <head> command (#49)

## [1.10.0] - 2026-03-24

### Features
- SARIF output format and init --hooks command (#47)

## [1.9.0] - 2026-03-22

### Bug Fixes
- Correct binary download URL, Windows zip, and latest version resolution (#44)


### Documentation
- Add quadrant and activity risk documentation


### Features
- Add Vue SFC (.vue) language support (#46)

## [1.8.1] - 2026-03-15

### Performance
- Parallelize per-function touch cache misses with rayon (#42)

## [1.8.0] - 2026-03-14

### Bug Fixes
- Use (i*n)/k for full-range pivot sampling in approx betweenness


### Features
- Approximate betweenness centrality for large codebases
- Warn on minified and vendored files during analysis


### Performance
- Fix O(N³) BFS queue, O(N×E) fan-in, and redundant sorts

## [1.7.1] - 2026-03-13

### Bug Fixes
- Add missing NS metric pill to report legend (#39)


### Documentation
- Add Cloudflare Pages deploy + document scatter plot
- Document scatter plot and update HTML report improvements tracker

## [1.7.0] - 2026-03-12

### Features
- Add Risk Landscape scatter plot to HTML reports (#38)

## [1.6.1] - 2026-03-12

### Bug Fixes
- Use < threshold notation for band legend upper bounds (#36)

## [1.6.0] - 2026-03-10

### Features
- Improve HTML report UX with glossary, band legend, and source link support (#34)

## [1.5.0] - 2026-03-07

### Bug Fixes
- Guard cliff.toml footer against null version for unreleased


### Features
- Add security scanning, SECURITY.md, and supply chain docs (#32)

## [1.4.0] - 2026-03-06

### Performance
- Parallelize file analysis with rayon (#30)

## [1.3.0] - 2026-03-06

### Documentation
- Add Homebrew install instructions for macOS
- Restructure installation page, lead with brew for macOS


### Features
- Add analysis progress bar with ETA (#29)

## [1.2.2] - 2026-03-04

### Documentation
- Add --all-functions, fix trends formats, update roadmap version (#27)

## [1.2.1] - 2026-03-03

### Bug Fixes
- CFG orphaned nodes, CC inflation, and JSX/decorator parser support (#25)

## [1.2.0] - 2026-02-28

### Features
- Pattern detection — 13 code smell labels with HTML dashboard and --explain-patterns (#24)

## [1.1.1] - 2026-02-24

### Bug Fixes
- Explicitly trigger release.yml after cargo-release pushes tag
- Prevent CFG builder panic and orphaned join nodes on dead code (#23)
- Use RELEASE_PAT so tag push triggers release.yml

## [1.1.0] - 2026-02-22

### Bug Fixes
- Replace yourorg placeholder with correct org in action
- Remove invalid post-release-commit-message from release.toml
- Remove invalid workspace field from release.toml
- Commit Cargo.lock changes before cargo-release runs


### Documentation
- No-sudo install, user-land path, fix broken links; add deploy config
- Update install instructions to use install.sh
- Use GitHub raw URL for install.sh, redirect hotspots.dev later
- Add website/docs links to README header
- Add hotspots.dev link to nav and footer
- Note GitHub Action is not yet available
- Sync README with docs content
- Add logo as favicon and README header
- Add OG and Twitter Card meta tags to docs site


### Features
- Add install.sh for one-line install
- Add progress bar, fix CFG traversal for typescript (#22)

## [1.0.0] - 2026-02-21

### Features
- Add GitHub Action for CI/CD integration (Task 2.1) (#3)

[1.27.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.27.0
[1.26.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.26.0
[1.25.3]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.25.3
[1.25.2]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.25.2
[1.25.1]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.25.1
[1.25.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.25.0
[1.24.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.24.0
[1.23.3]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.23.3
[1.23.2]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.23.2
[1.23.1]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.23.1
[1.23.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.23.0
[1.22.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.22.0
[1.21.1]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.21.1
[1.20.1]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.20.1
[1.20.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.20.0
[1.18.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.18.0
[1.17.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.17.0
[1.16.1]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.16.1
[1.16.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.16.0
[1.15.1]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.15.1
[1.15.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.15.0
[1.14.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.14.0
[1.13.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.13.0
[1.12.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.12.0
[1.11.1]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.11.1
[1.11.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.11.0
[1.10.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.10.0
[1.9.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.9.0
[1.8.1]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.8.1
[1.8.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.8.0
[1.7.1]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.7.1
[1.7.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.7.0
[1.6.1]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.6.1
[1.6.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.6.0
[1.5.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.5.0
[1.4.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.4.0
[1.3.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.3.0
[1.2.2]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.2.2
[1.2.1]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.2.1
[1.2.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.2.0
[1.1.1]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.1.1
[1.1.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.1.0
[1.0.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.0.0

