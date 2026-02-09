# Hotspots Codebase – Improvements Report

This report summarizes high‑impact issues and concrete recommendations based on codebase review. Items are grouped by priority with file references for fast action.

**Last Updated:** 2026-02-07 (Post Rust implementation)

## Summary
- Strong architecture with deterministic outputs and comprehensive test coverage (209 tests)
- Full multi-language support: TypeScript, JavaScript, React, Go, and Rust (production-ready)
- Main gap: branding/config consistency causing user confusion

## High‑Impact Issues

### 1. Branding/Config Consistency (P0 - User-Facing Confusion)

**Problem:** Mixed naming between "faultline" and "hotspots" creates confusion:

- **CLI Command Name**
  - Binary is named `hotspots` but Clap declares command name as `faultline`
  - File: `hotspots-cli/src/main.rs` (struct `Cli` annotations)

- **Config Discovery**
  - Code looks for `.faultlinerc.json` and `faultline.config.json`
  - Documentation references `.hotspotsrc.json` and `hotspots.config.json`
  - package.json uses `"faultline"` key
  - Files: `hotspots-core/src/config.rs`

- **Snapshot Storage**
  - Code uses `.faultline/` directory
  - Documentation may reference `.hotspots/`
  - File: `hotspots-core/src/snapshot.rs`

- **GitHub Action Documentation**
  - References `yourorg/hotspots` placeholder
  - Should reference actual published action location
  - File: `action/README.md`, `README.md`

**Impact:** Users see inconsistent naming in CLI help, config files, and documentation.

## Recommendations by Priority

### P0 - Standardize to "hotspots" Everywhere

**Rationale:** This is a breaking change, so should be done as a major version bump with clear migration guide.

**Changes Required:**

1. **CLI Command Name**
   - File: `hotspots-cli/src/main.rs`
   - Change `#[command(name = "faultline", ...)]` to `#[command(name = "hotspots", ...)]`
   - Update about text to match README positioning

2. **Config File Discovery**
   - File: `hotspots-core/src/config.rs`
   - Search for: `.hotspotsrc.json` → `hotspots.config.json` → `package.json` key `"hotspots"`
   - Update all doc comments and error messages
   - Add deprecation warning for old `.faultlinerc.json` format (read but warn)

3. **Snapshot Directory**
   - File: `hotspots-core/src/snapshot.rs`
   - Change from `.faultline/` to `.hotspots/`
   - Consider migration path: check for old directory and auto-migrate or warn

4. **Documentation Updates**
   - Files: `README.md`, `docs/USAGE.md`, `action/README.md`
   - Replace all references to `faultline` with `hotspots`
   - Ensure examples use correct config file names
   - Update GitHub Action `uses:` reference to actual published location

5. **Migration Guide**
   - Create `docs/MIGRATION.md` explaining:
     - Rename `.faultlinerc.json` → `.hotspotsrc.json`
     - Update package.json `"faultline"` → `"hotspots"` key
     - `.faultline/` directory → `.hotspots/`
     - CLI command name change (if referenced in scripts)

### P1 - File Discovery Improvements

**File:** `hotspots-core/src/lib.rs` (`collect_source_files_recursive`)

**Current exclusions:** `node_modules`, `.git`, hidden files

**Add to exclusions:**
- `dist` - Build output directory
- `build` - Alternative build directory
- `out` - Another common output directory
- `coverage` - Test coverage artifacts
- `target` - Rust build artifacts (already implicitly excluded but be explicit)

**Symlink handling:**
- Use `symlink_metadata()` instead of `metadata()` to detect symlinks
- Don't follow symlinks to prevent infinite loops
- Log skipped symlinks at debug level

### P1 - MCP Server Cross-Platform Improvements

**File:** `packages/mcp-server/src/analyze.ts`

**1. Windows Binary Lookup:**
```typescript
// Current: only uses 'which' (Unix/Mac)
// Add: Windows support with 'where' command
const command = process.platform === 'win32' ? 'where' : 'which';
```

**2. Error Detection:**
```typescript
// Current: checks stderr.includes('error')
// Better: rely on exit code
if (result.exitCode !== 0) {
  throw new Error(`Analysis failed: ${result.stderr}`);
}
```

**3. Output Shape Handling:**
- Handle both snapshot/delta mode objects and plain arrays
- Update `generateSummary()` to detect output type

### P2 - Nice to Have

**1. CLI Text Output Safety**
- File: `hotspots-core/src/render.rs`
- Use character-safe truncation for paths with Unicode
- Low priority since most paths are ASCII

**2. Best-Effort Analysis Mode**
- Consider adding `--continue-on-error` flag
- Log parse errors but continue with other files
- Useful for CI on partially broken codebases
- **Note:** Current fail-fast behavior ensures determinism, so this should be opt-in only

## What's Already Strong

### ✅ Multi-Language Support (Complete)
- **Rust support fully implemented** (as of 2026-02-07):
  - Full parser using `syn` crate
  - Complete CFG builder with all control flow constructs
  - Comprehensive metrics: CC, ND, FO, NS with Rust-specific features (?, unwrap, panic, match)
  - 6 test fixtures covering all language features
  - 5 golden file tests ensuring determinism
  - Full documentation in `language-support.md`
- **Go support** production-ready with goroutines, defer, select, channels
- **ECMAScript** (TypeScript/JavaScript/React) with full feature parity

### ✅ Test Coverage (Excellent)
- **209 total tests** with 100% pass rate
- Golden file tests for all languages ensure determinism
- Config discovery, snapshot handling, policy enforcement all tested
- Integration tests for git history, delta mode, suppression

### ✅ Determinism & Correctness
- Stable sort orders enforced across modules
- CFG validation catches construction errors
- Byte-for-byte reproducible output

### ✅ Architecture
- Clean language-agnostic traits (LanguageParser, CfgBuilder)
- Pluggable parsers for each language
- Well-separated concerns (parse → CFG → metrics → risk → policy)

## Implementation Priority

**Phase 1 (Breaking Changes - v2.0.0):**
1. Standardize all naming to "hotspots" (P0)
2. Create migration guide
3. Test migration path with real projects

**Phase 2 (Non-Breaking Improvements):**
1. File discovery improvements (P1)
2. MCP server cross-platform support (P1)

**Phase 3 (Future Enhancements):**
1. Text output Unicode safety (P2)
2. Continue-on-error mode (P2)

---

## Next Steps

1. Create GitHub issue for naming standardization
2. Plan v2.0.0 breaking change
3. Implement file discovery and MCP improvements (non-breaking)
4. Update documentation to match actual published GitHub Action location

**Note:** Rust language support is now production-ready and does not require further work beyond normal maintenance and potential feature additions (closures, if let/while let in future).
