# Publishing the Hotspots GitHub Action

This guide covers how to publish and distribute the Hotspots GitHub Action.

## Pre-Publishing Checklist

- [ ] All tests pass (`npm run all` in `action/`)
- [ ] Action is built and bundled (`dist/index.js` committed)
- [ ] Documentation is complete (`action/README.md`, main `README.md`)
- [ ] Version numbers are updated (`Cargo.toml`, `action/package.json`)
- [ ] CHANGELOG.md is updated

## Publishing Steps

### 1. Build the Action

```bash
cd action
npm install
npm run package
```

This creates `dist/index.js` which must be committed to git.

### 2. Commit the Distribution

```bash
git add action/dist/
git commit -m "chore: build action dist for v1.0.0"
```

**Important:** The `dist/` directory MUST be committed. GitHub Actions runs the code from `dist/index.js`, not from `src/`.

### 3. Create a Release Tag

```bash
# Create and push the version tag
git tag v1.0.0
git push origin v1.0.0

# This triggers the release workflow which builds binaries
```

### 4. Update Major Version Tag

After releasing, update the major version tag so users can use `@v1`:

```bash
# Force update the v1 tag to point to v1.0.0
git tag -fa v1 -m "Update v1 to v1.0.0"
git push origin v1 --force
```

Now users can reference the action with:
```yaml
- uses: yourorg/hotspots@v1  # Always uses latest v1.x.x
```

### 5. Verify Release

Check that the release workflow completed:
1. Go to Actions tab → Release workflow
2. Verify all platform binaries were built
3. Check the Releases page for the new release
4. Download and test a binary

### 6. Test the Published Action

Create a test repository and add:

```yaml
# .github/workflows/test-published.yml
name: Test Published Action
on: [push]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: yourorg/hotspots@v1.0.0  # Test specific version
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

Verify:
- Action runs successfully
- Binary downloads correctly
- Analysis completes
- Outputs are correct

## Version Numbering Strategy

### Semantic Versioning

- **v1.0.0**: Initial stable release
- **v1.1.0**: New features (backward compatible)
- **v1.0.1**: Bug fixes
- **v2.0.0**: Breaking changes

### Major Version Tags

Users should reference major version tags:

```yaml
- uses: yourorg/hotspots@v1  # Recommended - auto-updates to latest v1.x.x
- uses: yourorg/hotspots@v1.0.0  # Pin to specific version
```

After each release, update the major version tag:

```bash
# After releasing v1.2.0
git tag -fa v1 -m "Update v1 to v1.2.0"
git push origin v1 --force
```

## Release Workflow

The `.github/workflows/release.yml` automates:

1. **Building** for all platforms:
   - Linux x64
   - macOS x64 (Intel)
   - macOS ARM64 (Apple Silicon)
   - Windows x64

2. **Creating GitHub Release** with:
   - All platform binaries
   - Auto-generated release notes
   - Links to documentation

3. **Uploading Artifacts** that the action downloads at runtime

## Distribution Methods

### 1. GitHub Action (Primary)

Users add to their workflows:

```yaml
- uses: yourorg/hotspots@v1
```

The action automatically:
- Downloads the correct binary for the platform
- Caches it for future runs
- Runs hotspots with specified options

### 2. Direct Binary Download

Users can download from releases:

```bash
wget https://github.com/yourorg/hotspots/releases/latest/download/hotspots-linux-x64.tar.gz
```

### 3. Cargo Installation

Users with Rust can install from git:

```bash
cargo install --git https://github.com/yourorg/hotspots --tag v1.0.0
```

## Marketplace Publishing (Optional)

To list on GitHub Marketplace:

1. Add marketplace metadata to `action/action.yml`:
   ```yaml
   name: 'Hotspots Complexity Analysis'
   description: 'Block complexity regressions in TypeScript/JavaScript'
   branding:
     icon: 'alert-triangle'
     color: 'orange'
   ```

2. Go to repository Settings → Actions → General
3. Check "Allow actions and reusable workflows"
4. In the Marketplace section, click "List on Marketplace"
5. Fill out the form:
   - Category: Code Quality
   - Tags: complexity, typescript, javascript, ci-cd, code-quality

6. Submit for review

Once approved, the action appears at:
`https://github.com/marketplace/actions/hotspots-complexity-analysis`

## Updating After Release

### Patch Release (Bug Fix)

```bash
# Fix the bug
git commit -m "fix: resolve binary download issue"

# Update version
npm version patch  # Updates package.json to 1.0.1

# Rebuild action
npm run package
git add dist/
git commit -m "chore: rebuild action dist"

# Release
git tag v1.0.1
git push origin v1.0.1

# Update major version tag
git tag -fa v1 -m "Update v1 to v1.0.1"
git push origin v1 --force
```

### Minor Release (New Feature)

```bash
# Add the feature
git commit -m "feat: add custom threshold support"

# Update version
npm version minor  # Updates to 1.1.0

# Rebuild and release (same as patch)
npm run package
git add dist/
git commit -m "chore: rebuild action dist"
git tag v1.1.0
git push origin v1.1.0
git tag -fa v1 -m "Update v1 to v1.1.0"
git push origin v1 --force
```

### Major Release (Breaking Change)

```bash
# Make breaking changes
git commit -m "feat!: redesign policy engine (BREAKING)"

# Update version
npm version major  # Updates to 2.0.0

# Rebuild and release
npm run package
git add dist/
git commit -m "chore: rebuild action dist"
git tag v2.0.0
git push origin v2.0.0

# Create new major version tag (don't force update v1!)
git tag v2 -m "Initial v2 release"
git push origin v2

# Update README with migration guide
```

## Deprecating Old Versions

If v1 has a critical security issue:

1. Create a fix in v1.x branch
2. Release v1.x.y with the fix
3. Add deprecation notice to old release pages
4. Update v1 tag to point to the fixed version

Do NOT delete old releases - users may have pinned to specific versions.

## Troubleshooting

### Action Not Finding Binary

**Problem:** `Error: Unable to locate executable file: hotspots`

**Solution:** Check that:
1. Release workflow completed successfully
2. Binaries are attached to the release
3. File names match the expected pattern:
   - `hotspots-linux-x64.tar.gz`
   - `hotspots-darwin-x64.tar.gz`
   - `hotspots-darwin-arm64.tar.gz`
   - `hotspots-win32-x64.zip`

### dist/ Not Up to Date

**Problem:** Changes not reflected in action

**Solution:** Always rebuild before releasing:
```bash
cd action
npm run package
git add dist/
git commit -m "chore: rebuild action dist"
```

### Major Version Tag Not Working

**Problem:** Users get old version when using `@v1`

**Solution:** Force update the major tag:
```bash
git tag -fa v1 -m "Update v1 to latest"
git push origin v1 --force
```

## Support and Feedback

After publishing:

1. **Monitor Issues:** Watch for installation problems
2. **Update Docs:** Add FAQs based on user questions
3. **Iterate:** Collect feedback and improve
4. **Announce:** Share on Twitter, Reddit, etc.

## Checklist for First Release

- [ ] Build action: `npm run package`
- [ ] Commit dist: `git add action/dist && git commit`
- [ ] Tag release: `git tag v1.0.0`
- [ ] Push tag: `git push origin v1.0.0`
- [ ] Wait for release workflow to complete
- [ ] Verify binaries are attached to release
- [ ] Create major version tag: `git tag v1 && git push origin v1`
- [ ] Test in a sample repository
- [ ] Update documentation with real usage examples
- [ ] Announce the release!

## Next Steps

After v1.0.0 is published:
- [ ] Add to GitHub Marketplace
- [ ] Create example repositories
- [ ] Write blog post about the tool
- [ ] Share on social media
- [ ] Collect feedback and iterate
