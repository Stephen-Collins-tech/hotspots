# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

