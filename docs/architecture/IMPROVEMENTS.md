# Hotspots Architecture Improvements

**Version:** 1.0  
**Last Updated:** 2026-02-15  
**Status:** Proposal

---

## Overview

This document outlines potential architectural improvements to Hotspots, organized by priority and impact. These improvements would enhance performance, maintainability, extensibility, and scalability without breaking existing functionality.

---

## High Priority: Performance & Scalability

### 1. Parallel File Analysis

**Current State:**
- Analysis is single-threaded
- Files are processed sequentially
- No parallelization of independent operations

**Proposed Improvement:**
- Use `rayon` for parallel file processing
- Parallelize independent stages:
  - File parsing (per-file, no shared state)
  - Function discovery (per-file)
  - CFG building (per-function)
  - Metric extraction (per-function)

**Implementation:**
```rust
// Parallel file analysis
let reports: Vec<_> = files
    .par_iter()
    .map(|file| analyze_file(file, config))
    .collect();
```

**Benefits:**
- 4-8x speedup on multi-core systems
- Scales with CPU cores
- Minimal code changes (rayon's `par_iter`)

**Challenges:**
- Must maintain deterministic ordering for output
- Git operations still sequential (external dependency)
- Memory usage increases with parallelism

**Estimated Impact:** 4-8x faster for large repos

---

### 2. Incremental Analysis & Caching

**Current State:**
- Full analysis runs every time
- No caching of parsed ASTs or CFGs
- No change detection

**Proposed Improvement:**
- Cache parsed ASTs per file (hash-based)
- Cache CFGs per function (content hash)
- Only re-analyze changed files/functions
- Store cache in `.hotspots/cache/`

**Implementation:**
```rust
struct AnalysisCache {
    ast_cache: HashMap<PathBuf, (u64, ParsedModule)>, // hash -> AST
    cfg_cache: HashMap<FunctionId, (u64, Cfg)>,      // hash -> CFG
}

fn analyze_with_cache(file: &Path, cache: &mut AnalysisCache) -> Result<Vec<Report>> {
    let content_hash = hash_file(file)?;
    if let Some((cached_hash, ast)) = cache.ast_cache.get(file) {
        if *cached_hash == content_hash {
            return Ok(use_cached_ast(ast));
        }
    }
    // Parse and cache
    let ast = parse(file)?;
    cache.ast_cache.insert(file.clone(), (content_hash, ast));
    // ...
}
```

**Benefits:**
- 10-100x faster for incremental changes
- Reduces CPU usage
- Enables faster CI feedback

**Challenges:**
- Cache invalidation strategy
- Cache size management
- Determinism must be preserved

**Estimated Impact:** 10-100x faster for incremental runs

---

### 3. Batched Git Operations

**Current State:**
- Each file's touch metrics require separate `git log` calls
- Sequential git operations
- No caching of git results

**Proposed Improvement:**
- Batch git operations: `git log --since=X --until=Y -- file1 file2 file3 ...`
- Cache git results per commit SHA
- Parallel git operations where possible

**Implementation:**
```rust
fn batch_touch_metrics(
    files: &[PathBuf],
    as_of: i64,
) -> Result<HashMap<PathBuf, TouchMetrics>> {
    let output = git(&[
        "log",
        &format!("--since={}", as_of - 30*24*60*60),
        &format!("--until={}", as_of),
        "--oneline",
        "--",
        // All files at once
    ])?;
    // Parse output and group by file
}
```

**Benefits:**
- 10-50x faster git operations for many files
- Reduces git process overhead
- Better for large repos

**Challenges:**
- Git command line length limits
- Output parsing complexity
- Still sequential (git limitation)

**Estimated Impact:** 10-50x faster git operations

---

### 4. Optimize Call Graph Algorithms

**Current State:**
- PageRank: O(V * E * iterations) ≈ O(V * E * 20)
- Betweenness: O(V * E) (Brandes algorithm)
- Both run on every snapshot

**Proposed Improvement:**
- Incremental PageRank (only recompute changed nodes)
- Approximate Betweenness (sampling-based)
- Skip graph metrics if call graph unchanged
- Use sparse matrix representations

**Implementation:**
```rust
// Incremental PageRank
fn incremental_pagerank(
    graph: &CallGraph,
    previous_scores: &HashMap<FunctionId, f64>,
    changed_nodes: &HashSet<FunctionId>,
) -> HashMap<FunctionId, f64> {
    // Only recompute affected nodes
}
```

**Benefits:**
- 5-10x faster for large call graphs
- Scales better with repo size
- Enables real-time analysis

**Challenges:**
- Algorithm correctness
- Maintaining determinism
- Testing complexity

**Estimated Impact:** 5-10x faster call graph computation

---

## Medium Priority: Architecture & Maintainability

### 5. Plugin System for Metrics

**Current State:**
- Metrics hardcoded in `metrics.rs`
- Adding new metrics requires core changes
- No way to extend metrics per-project

**Proposed Improvement:**
- Trait-based metric system
- Plugin registry for custom metrics
- Config-driven metric selection

**Implementation:**
```rust
trait MetricExtractor {
    fn name(&self) -> &str;
    fn extract(&self, function: &FunctionNode, cfg: &Cfg) -> f64;
    fn weight(&self) -> f64;
}

struct MetricRegistry {
    extractors: Vec<Box<dyn MetricExtractor>>,
}

// Custom metric example
struct CustomMetric {
    name: String,
    extractor: fn(&FunctionNode, &Cfg) -> f64,
    weight: f64,
}
```

**Benefits:**
- Extensibility without core changes
- Project-specific metrics
- Community contributions

**Challenges:**
- Plugin API design
- Backward compatibility
- Performance overhead

**Estimated Impact:** High extensibility, low performance impact

---

### 6. Policy Trait System

**Current State:**
- Policy evaluation duplicated across 7 functions
- Manual status filtering and suppression checks
- Hard to add new policies

**Proposed Improvement:**
- Trait-based policy system
- Single evaluation loop
- Declarative policy definitions

**Implementation:**
```rust
trait Policy {
    fn id(&self) -> PolicyId;
    fn severity(&self) -> PolicySeverity;
    fn target_statuses(&self) -> &[FunctionStatus];
    fn evaluate(&self, entry: &FunctionDeltaEntry, config: &Config) -> Option<PolicyResult>;
}

struct PolicyRegistry {
    policies: Vec<Box<dyn Policy>>,
}

fn evaluate_all_policies(
    deltas: &[FunctionDeltaEntry],
    registry: &PolicyRegistry,
    config: &Config,
) -> PolicyResults {
    let mut results = PolicyResults::new();
    for entry in active_deltas(deltas) {
        for policy in &registry.policies {
            if policy.target_statuses().contains(&entry.status) {
                if let Some(result) = policy.evaluate(entry, config) {
                    results.add(result);
                }
            }
        }
    }
    results
}
```

**Benefits:**
- Eliminates duplication
- Easier to add policies
- Testable policy logic

**Challenges:**
- Migration from current system
- Performance (trait objects)
- Backward compatibility

**Estimated Impact:** Better maintainability, minimal performance impact

---

### 7. Dependency Injection for Testability

**Current State:**
- Tight coupling to file system, git, and external dependencies
- Hard to test without real filesystem/git
- No way to mock dependencies

**Proposed Improvement:**
- Trait-based abstractions for I/O
- Dependency injection container
- Test doubles for git/filesystem

**Implementation:**
```rust
trait FileSystem {
    fn read_file(&self, path: &Path) -> Result<String>;
    fn write_file(&self, path: &Path, content: &str) -> Result<()>;
}

trait GitOperations {
    fn get_commit_info(&self, sha: &str) -> Result<CommitInfo>;
    fn get_churn(&self, sha: &str) -> Result<HashMap<String, FileChurn>>;
}

struct AnalysisContext {
    fs: Box<dyn FileSystem>,
    git: Box<dyn GitOperations>,
    config: ResolvedConfig,
}
```

**Benefits:**
- Unit tests without real filesystem
- Mock git operations
- Better test coverage

**Challenges:**
- Large refactoring
- Trait object overhead
- Migration complexity

**Estimated Impact:** Better testability, minimal runtime impact

---

### 8. Streaming Output for Large Repos

**Current State:**
- All results collected in memory
- JSON/HTML generated at end
- Memory usage scales with repo size

**Proposed Improvement:**
- Stream results as they're computed
- Incremental JSON/HTML generation
- Support for very large repos

**Implementation:**
```rust
trait OutputStream {
    fn write_function(&mut self, report: &FunctionRiskReport) -> Result<()>;
    fn finish(&mut self) -> Result<()>;
}

struct StreamingJsonOutput {
    writer: BufWriter<File>,
    first: bool,
}

impl OutputStream for StreamingJsonOutput {
    fn write_function(&mut self, report: &FunctionRiskReport) -> Result<()> {
        if !self.first {
            self.writer.write_all(b",\n")?;
        }
        serde_json::to_writer(&mut self.writer, report)?;
        self.first = false;
        Ok(())
    }
}
```

**Benefits:**
- Constant memory usage
- Handles very large repos
- Faster time-to-first-result

**Challenges:**
- Output format changes
- HTML streaming complexity
- Backward compatibility

**Estimated Impact:** Enables analysis of very large repos

---

## Lower Priority: Quality of Life

### 9. Language Plugin System

**Current State:**
- Languages hardcoded in core
- Adding language requires core changes
- No way to extend language support externally

**Proposed Improvement:**
- Dynamic language registration
- Plugin-based language support
- External language implementations

**Implementation:**
```rust
trait LanguagePlugin {
    fn name(&self) -> &str;
    fn extensions(&self) -> &[&str];
    fn parser(&self) -> Box<dyn LanguageParser>;
    fn cfg_builder(&self) -> Box<dyn CfgBuilder>;
}

struct LanguageRegistry {
    languages: HashMap<String, Box<dyn LanguagePlugin>>,
}
```

**Benefits:**
- Community language contributions
- No core changes for new languages
- Experimental language support

**Challenges:**
- Plugin API complexity
- Version compatibility
- Security considerations

**Estimated Impact:** Enables community language support

---

### 10. Structured Error Types

**Current State:**
- Uses `anyhow::Result` everywhere
- Generic error messages
- Hard to handle specific error cases

**Proposed Improvement:**
- Domain-specific error types
- Structured error information
- Better error recovery

**Implementation:**
```rust
#[derive(Debug, thiserror::Error)]
enum AnalysisError {
    #[error("Parse error in {file}: {message}")]
    ParseError { file: PathBuf, message: String },
    #[error("Git error: {0}")]
    GitError(#[from] GitError),
    #[error("Config error: {0}")]
    ConfigError(#[from] ConfigError),
}

type AnalysisResult<T> = Result<T, AnalysisError>;
```

**Benefits:**
- Better error messages
- Programmatic error handling
- Error recovery strategies

**Challenges:**
- Migration effort
- Breaking changes
- Error type proliferation

**Estimated Impact:** Better error handling and debugging

---

### 11. AST Storage Optimization

**Current State:**
- Full AST stored for all functions
- Tree-sitter nodes require source + node ID
- Memory usage scales with codebase size

**Proposed Improvement:**
- Lazy AST parsing (parse on demand)
- Compact AST representation
- Shared AST nodes where possible

**Implementation:**
```rust
enum LazyFunctionBody {
    Parsed(FunctionBody),
    Unparsed { source: String, language: Language },
}

impl LazyFunctionBody {
    fn parse(&mut self) -> Result<&FunctionBody> {
        match self {
            Self::Parsed(body) => Ok(body),
            Self::Unparsed { source, language } => {
                let parsed = parse_function(source, language)?;
                *self = Self::Parsed(parsed);
                Ok(match self {
                    Self::Parsed(body) => body,
                    _ => unreachable!(),
                })
            }
        }
    }
}
```

**Benefits:**
- Reduced memory usage
- Faster initial analysis
- Better for large repos

**Challenges:**
- Complexity increase
- Parsing overhead on access
- Cache invalidation

**Estimated Impact:** 30-50% memory reduction

---

### 12. Configuration Validation & Schema

**Current State:**
- Config validated at runtime
- No schema documentation
- Easy to make config mistakes

**Proposed Improvement:**
- JSON Schema for config
- Config validation with clear errors
- IDE autocomplete support

**Implementation:**
```json
{
  "$schema": "https://hotspots.dev/schemas/config-v1.json",
  "include": ["src/**/*.ts"],
  "thresholds": {
    "moderate": 3.0
  }
}
```

**Benefits:**
- Better developer experience
- Catch errors early
- Self-documenting config

**Challenges:**
- Schema maintenance
- Version compatibility
- Tooling support

**Estimated Impact:** Better UX, fewer config errors

---

## Implementation Roadmap

### Phase 1: Performance (3-6 months)
1. **Parallel file analysis** (2-3 weeks)
   - Add rayon dependency
   - Parallelize file processing
   - Maintain deterministic ordering

2. **Batched git operations** (1-2 weeks)
   - Batch touch metrics queries
   - Cache git results
   - Measure performance gains

3. **Incremental analysis** (4-6 weeks)
   - Implement cache system
   - Change detection
   - Cache invalidation

### Phase 2: Architecture (6-9 months)
4. **Policy trait system** (2-3 weeks)
   - Define Policy trait
   - Migrate existing policies
   - Add tests

5. **Dependency injection** (4-6 weeks)
   - Define I/O traits
   - Refactor core to use traits
   - Add test doubles

6. **Streaming output** (2-3 weeks)
   - Implement streaming JSON
   - Streaming HTML (if feasible)
   - Backward compatibility

### Phase 3: Extensibility (9-12 months)
7. **Metric plugin system** (3-4 weeks)
   - Define MetricExtractor trait
   - Plugin registry
   - Documentation

8. **Language plugin system** (6-8 weeks)
   - Define LanguagePlugin trait
   - Dynamic registration
   - Example plugins

9. **Structured errors** (2-3 weeks)
   - Define error types
   - Migrate error handling
   - Update documentation

---

## Trade-offs & Considerations

### Performance vs. Simplicity
- Parallelization adds complexity but significant speedup
- Caching adds complexity but enables incremental analysis
- **Recommendation:** Start with parallelization, add caching later

### Extensibility vs. Performance
- Plugin systems add indirection overhead
- Trait objects have vtable cost
- **Recommendation:** Use generics where possible, traits where necessary

### Memory vs. Speed
- Streaming reduces memory but adds complexity
- AST caching speeds up but uses more memory
- **Recommendation:** Make configurable, default to balanced

### Backward Compatibility
- Most improvements can be additive
- Some require breaking changes (error types, config schema)
- **Recommendation:** Version APIs, provide migration guides

---

## Success Metrics

### Performance
- **Target:** 5-10x faster for large repos (>10k functions)
- **Measure:** Analysis time, memory usage, CPU utilization

### Maintainability
- **Target:** Reduce code duplication by 50%
- **Measure:** Lines of code, cyclomatic complexity, test coverage

### Extensibility
- **Target:** Add new metric/language in <100 lines
- **Measure:** Plugin API complexity, documentation quality

### Scalability
- **Target:** Handle repos with 100k+ functions
- **Measure:** Analysis time, memory usage, success rate

---

## References

- [Current Architecture](./ARCHITECTURE.md)
- [Performance Bottlenecks](../reference/limitations.md#performance)
- [Codebase Analysis](../../ANALYSIS.md)
- [Improvement Tasks](../../CODEBASE_IMPROVEMENTS.md)

---

**Document Status:** Proposal  
**Next Review:** After Phase 1 completion  
**Questions?** Open an issue or see `docs/` for more details.
