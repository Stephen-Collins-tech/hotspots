# Version Management

Hotspots uses git tags for versioning, with automatic fallback to `CARGO_PKG_VERSION`.

## Current Implementation

We use a **build script** (`hotspots-cli/build.rs`) that:
- Runs `git describe --tags --always --dirty` at build time
- Extracts version from git tags (e.g., `v0.1.0` → `0.1.0`)
- Falls back to `CARGO_PKG_VERSION` if git is unavailable
- Handles dirty working directories gracefully

## Version Format

- **Tagged release**: `0.1.0` (from git tag `v0.1.0`)
- **Development build**: `0.1.0-b1ea582-dirty` (base version + commit + dirty flag)
- **No git**: `0.1.0` (from Cargo.toml)

## Alternative Approaches

### Option 1: Current (Build Script) ✅ **Recommended for simplicity**
**Pros:**
- No additional dependencies
- Works in most environments
- Simple and maintainable

**Cons:**
- Requires git at build time
- May fail in some CI/CD environments without git

### Option 2: `vergen` crate
**Pros:**
- Industry standard (de-facto standard)
- Handles edge cases well
- More features (commit date, branch, etc.)

**Cons:**
- Adds a build dependency
- Slightly more complex setup

**Usage:**
```toml
[build-dependencies]
vergen = { version = "9", features = ["git", "gitcl"] }
```

### Option 3: `git-version` crate
**Pros:**
- Lightweight
- Simple API
- Good for basic use cases

**Cons:**
- Less feature-rich than vergen
- Still adds a dependency

**Usage:**
```toml
[build-dependencies]
git-version = "0.3"
```

## Recommendation

**For this project:** The current build script approach is appropriate because:
1. We already use git extensively (snapshots, deltas, history)
2. Git is always available in our development workflow
3. No additional dependencies needed
4. Simple and maintainable

**For production releases:** Always build from a tagged commit:
```bash
git tag v0.2.0
git checkout v0.2.0
cargo build --release
# Version will be: 0.2.0
```

## Migration to `vergen` (if needed)

If you want to switch to `vergen` for better CI/CD compatibility:

1. Add to `Cargo.toml`:
```toml
[build-dependencies]
vergen = { version = "9", features = ["git", "gitcl"] }
```

2. Replace `build.rs` with:
```rust
use vergen::EmitBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    EmitBuilder::builder()
        .git_describe(true, true, None)
        .git_sha(true)
        .emit()?;
    Ok(())
}
```

3. Use in code:
```rust
#[command(version = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("VERGEN_GIT_DESCRIBE"),
    ")"
))]
```
