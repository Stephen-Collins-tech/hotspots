# Release Instructions for Faultline 0.0.1

This guide covers pushing to GitHub and publishing to crates.io.

---

## Prerequisites

- [ ] All changes committed (✅ Done: commit f6df5cd)
- [ ] Tag v0.0.1 created (✅ Done)
- [ ] Build passes: `cargo build --release` (✅ Done)
- [ ] Tests pass: `cargo test` and `make test-comprehensive`
- [ ] GitHub repository exists: https://github.com/Stephen-Collins-tech/faultline

---

## Step 1: Push to GitHub

### Push main branch and tag

```bash
# Push the main branch with all commits
git push origin main

# Push the v0.0.1 tag
git push origin v0.0.1
```

**Expected output:**
```
To github.com:Stephen-Collins-tech/faultline.git
 * [new branch]      main -> main
 * [new tag]         v0.0.1 -> v0.0.1
```

### Verify on GitHub

1. Visit: https://github.com/Stephen-Collins-tech/faultline
2. Check that files are visible (README, LICENSE-MIT, etc.)
3. Check that tag appears: https://github.com/Stephen-Collins-tech/faultline/tags

---

## Step 2: Create GitHub Release (Optional but Recommended)

1. Go to: https://github.com/Stephen-Collins-tech/faultline/releases/new
2. Select tag: `v0.0.1`
3. Release title: `v0.0.1 - Initial Release`
4. Copy description from CHANGELOG.md
5. Click "Publish release"

**Why do this:**
- Users can find releases easily
- Shows up in GitHub notifications
- Can attach binary builds later
- Professional appearance

---

## Step 3: Publish to crates.io

### 3.1 Get crates.io API Token (One-time setup)

1. Visit: https://crates.io/me
2. Click "Account Settings" → "API Tokens"
3. Click "New Token"
4. Name it (e.g., "faultline-publish")
5. Copy the token

### 3.2 Login to cargo

```bash
cargo login
# Paste your API token when prompted
```

**Note:** This saves the token to `~/.cargo/credentials` - you only need to do this once.

### 3.3 Dry Run (Recommended)

Test publishing without actually uploading:

```bash
# Test core library
cd faultline-core
cargo publish --dry-run

# Test CLI
cd ../faultline-cli
cargo publish --dry-run
```

**Look for:**
- No errors
- "Uploading" message (won't actually upload with --dry-run)
- Correct version (0.0.1)

### 3.4 Publish Core Library First

```bash
cd faultline-core
cargo publish
```

**Expected output:**
```
   Packaging faultline-core v0.0.1
   Verifying faultline-core v0.0.1
   Compiling faultline-core v0.0.1
    Finished dev [unoptimized + debuginfo] target(s)
   Uploading faultline-core v0.0.1
```

**Wait ~30-60 seconds** for the crate to become available on crates.io.

### 3.5 Publish CLI Binary

```bash
cd ../faultline-cli
cargo publish
```

**Expected output:**
```
   Packaging faultline-cli v0.0.1
   Verifying faultline-cli v0.0.1
   Compiling faultline-cli v0.0.1
    Finished dev [unoptimized + debuginfo] target(s)
   Uploading faultline-cli v0.0.1
```

### 3.6 Verify on crates.io

1. Core: https://crates.io/crates/faultline-core
2. CLI: https://crates.io/crates/faultline-cli

---

## Step 4: Test Installation

Wait 5-10 minutes for crates.io to fully propagate, then test:

```bash
# In a different directory
cargo install faultline-cli

# Verify it works
faultline --version
# Should output: faultline 0.0.1
```

---

## Troubleshooting

### Error: "failed to authenticate"
**Solution:** Run `cargo login` again with your API token.

### Error: "crate name is already taken"
**Solution:** Check if someone else published these names. If so, you'll need to:
- Rename the crates (e.g., `faultline-ts`, `faultline-analyzer`)
- Update Cargo.toml files
- Re-commit and re-tag

### Error: "repository URL does not exist"
**Solution:** Make sure you've pushed to GitHub first (Step 1).

### Error: "failed to verify package"
**Solution:**
- Check that `cargo build --release` works locally
- Make sure all dependencies are published crates (not path dependencies outside the workspace)

### CLI publish fails with "dependency not found"
**Solution:** Wait longer after publishing `faultline-core`. It can take 1-2 minutes to be available.

---

## Post-Release Checklist

After successful publishing:

- [ ] Verify crates appear on crates.io
- [ ] Test `cargo install faultline-cli` works
- [ ] Update GitHub repository description
- [ ] Add topics to GitHub repo: `typescript`, `static-analysis`, `rust`, `code-quality`
- [ ] Share on social media / Hacker News / Reddit r/rust (optional)
- [ ] Add shield badges to README (optional):
  ```markdown
  [![Crates.io](https://img.shields.io/crates/v/faultline-cli.svg)](https://crates.io/crates/faultline-cli)
  [![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT)
  ```

---

## Important Notes

### Cannot Unpublish
Once published to crates.io, you **cannot delete** a version. You can only:
- Yank it (hide from new installs, but existing users can still use it)
- Publish a new version

**Double-check before publishing!**

### Crate Name Ownership
After publishing, you own these crate names on crates.io. No one else can publish with the same name.

### Versioning
For future releases:
- Bug fixes: 0.0.2, 0.0.3, etc. (patch)
- New features: 0.1.0, 0.2.0, etc. (minor)
- Breaking changes: 1.0.0, 2.0.0, etc. (major)

Follow semantic versioning: https://semver.org/

---

## Need Help?

- Cargo book: https://doc.rust-lang.org/cargo/
- Publishing guide: https://doc.rust-lang.org/cargo/reference/publishing.html
- crates.io support: https://crates.io/policies

---

**Ready to release?** Follow steps 1-4 in order. Good luck! 🚀
