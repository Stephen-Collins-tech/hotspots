# Faultline Release Process

This document describes how to create releases for faultline, including building binaries for the GitHub Action.

## Prerequisites

- Rust toolchain installed
- GitHub CLI (`gh`) installed and authenticated
- Write access to the repository

## Release Checklist

- [ ] Update version in `Cargo.toml`
- [ ] Update version in `action/package.json`
- [ ] Update `CHANGELOG.md` with release notes
- [ ] Build binaries for all platforms
- [ ] Create GitHub release with binaries
- [ ] Test GitHub Action with new release
- [ ] Update documentation

## Building Release Binaries

### Method 1: Using GitHub Actions (Recommended)

We'll create a release workflow that builds for all platforms automatically.

### Method 2: Manual Cross-Compilation

#### Linux x86_64

```bash
cargo build --release --target x86_64-unknown-linux-gnu
strip target/x86_64-unknown-linux-gnu/release/faultline
tar -czf faultline-linux-x86_64.tar.gz \
  -C target/x86_64-unknown-linux-gnu/release faultline
```

#### macOS x86_64 (Intel)

```bash
cargo build --release --target x86_64-apple-darwin
strip target/x86_64-apple-darwin/release/faultline
tar -czf faultline-darwin-x86_64.tar.gz \
  -C target/x86_64-apple-darwin/release faultline
```

#### macOS ARM64 (Apple Silicon)

```bash
cargo build --release --target aarch64-apple-darwin
strip target/aarch64-apple-darwin/release/faultline
tar -czf faultline-darwin-aarch64.tar.gz \
  -C target/aarch64-apple-darwin/release faultline
```

#### Windows x86_64

```bash
cargo build --release --target x86_64-pc-windows-msvc
# Or cross-compile from Linux:
cargo build --release --target x86_64-pc-windows-gnu
zip faultline-windows-x86_64.zip \
  target/x86_64-pc-windows-*/release/faultline.exe
```

## Creating a Release

### 1. Update Version Numbers

```bash
# Update Cargo.toml
sed -i '' 's/version = ".*"/version = "1.0.0"/' Cargo.toml

# Update action/package.json
sed -i '' 's/"version": ".*"/"version": "1.0.0"/' action/package.json

# Update CHANGELOG.md
echo "## [1.0.0] - $(date +%Y-%m-%d)" >> CHANGELOG.md
```

### 2. Build Action

```bash
cd action
npm install
npm run package
git add dist/
cd ..
```

### 3. Commit and Tag

```bash
git add Cargo.toml action/package.json CHANGELOG.md action/dist/
git commit -m "chore: release v1.0.0"
git tag v1.0.0
git push origin main
git push origin v1.0.0
```

### 4. Create GitHub Release with Binaries

```bash
# Create release
gh release create v1.0.0 \
  --title "v1.0.0" \
  --notes-file CHANGELOG.md \
  faultline-linux-x86_64.tar.gz \
  faultline-darwin-x86_64.tar.gz \
  faultline-darwin-aarch64.tar.gz \
  faultline-windows-x86_64.zip
```

## Automated Release Workflow

Create `.github/workflows/release.yml`:

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

permissions:
  contents: write

jobs:
  build-binaries:
    name: Build ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            archive: tar.gz
          - os: macos-latest
            target: x86_64-apple-darwin
            archive: tar.gz
          - os: macos-latest
            target: aarch64-apple-darwin
            archive: tar.gz
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            archive: zip

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Build
        run: cargo build --release --target ${{ matrix.target }} --bin faultline

      - name: Create archive (Unix)
        if: matrix.archive == 'tar.gz'
        run: |
          cd target/${{ matrix.target }}/release
          tar -czf ../../../faultline-${{ matrix.target }}.${{ matrix.archive }} faultline
          cd ../../..

      - name: Create archive (Windows)
        if: matrix.archive == 'zip'
        shell: pwsh
        run: |
          cd target/${{ matrix.target }}/release
          Compress-Archive -Path faultline.exe -DestinationPath ../../../faultline-${{ matrix.target }}.${{ matrix.archive }}
          cd ../../..

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: faultline-${{ matrix.target }}
          path: faultline-${{ matrix.target }}.${{ matrix.archive }}

  create-release:
    name: Create Release
    needs: build-binaries
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Download artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts

      - name: Create Release
        uses: softprops/action-gh-release@v1
        with:
          files: artifacts/**/*
          generate_release_notes: true
          draft: false
          prerelease: false
```

## Post-Release Testing

After creating a release, test the GitHub Action:

```yaml
# In a test repository
- uses: yourorg/faultline@v1.0.0
```

Verify:
- [ ] Binary downloads successfully
- [ ] Analysis runs correctly
- [ ] PR comments are posted
- [ ] HTML report is generated
- [ ] Job summary is displayed

## Version Numbering

We follow [Semantic Versioning](https://semver.org/):

- **MAJOR** (1.0.0): Breaking API changes
- **MINOR** (0.1.0): New features, backwards compatible
- **PATCH** (0.0.1): Bug fixes, backwards compatible

## Release Cadence

- **Patch releases**: As needed for bug fixes
- **Minor releases**: Monthly for new features
- **Major releases**: Annually or when breaking changes are necessary

## Changelog Format

Follow [Keep a Changelog](https://keepachangelog.com/):

```markdown
## [1.0.0] - 2026-02-04

### Added
- GitHub Action for CI/CD integration
- HTML report generation
- Proactive warning system

### Changed
- Improved policy engine performance

### Fixed
- CFG routing for break/continue statements
```

## Rolling Back a Release

If a release has critical bugs:

```bash
# Delete the release
gh release delete v1.0.0 --yes

# Delete the tag
git tag -d v1.0.0
git push origin :refs/tags/v1.0.0

# Create a new patch release with fixes
```

## Updating the Action After Release

Users can reference the action by major version:

```yaml
- uses: yourorg/faultline@v1  # Automatically uses latest v1.x.x
```

To update the major version pointer:

```bash
git tag -fa v1 -m "Update v1 to v1.2.0"
git push origin v1 --force
```
