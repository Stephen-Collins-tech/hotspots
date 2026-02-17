# Hotspots Architecture Improvements Summary

**Date:** 2026-02-15  
**Status:** Proposal  
**Full Document:** [IMPROVEMENTS.md](./IMPROVEMENTS.md)

---

## Executive Summary

This report outlines 12 architectural improvements to Hotspots, organized by priority and impact. The highest-impact improvements focus on performance and scalability, with potential for 10-100x speedup for typical workflows. Medium-priority improvements enhance maintainability and extensibility, while lower-priority items improve quality of life and developer experience.

---

## High Priority: Performance & Scalability

### 1. Parallel File Analysis
- **Impact:** 4-8x speedup on multi-core systems
- **Effort:** 2-3 weeks
- **Approach:** Use `rayon` to parallelize independent file processing
- **Challenge:** Maintain deterministic ordering for output

### 2. Incremental Analysis & Caching
- **Impact:** 10-100x faster for incremental changes
- **Effort:** 4-6 weeks
- **Approach:** Cache parsed ASTs and CFGs, only re-analyze changed code
- **Challenge:** Cache invalidation strategy and size management

### 3. Batched Git Operations
- **Impact:** 10-50x faster git operations for many files
- **Effort:** 1-2 weeks
- **Approach:** Batch multiple file queries into single git commands
- **Challenge:** Git command line length limits

### 4. Optimize Call Graph Algorithms
- **Impact:** 5-10x faster for large call graphs
- **Effort:** 3-4 weeks
- **Approach:** Incremental PageRank, approximate Betweenness, sparse matrices
- **Challenge:** Algorithm correctness and determinism

**Combined Impact:** 10-100x faster analysis for typical incremental workflows

---

## Medium Priority: Architecture & Maintainability

### 5. Plugin System for Metrics
- **Impact:** High extensibility, low performance impact
- **Effort:** 3-4 weeks
- **Approach:** Trait-based metric system with plugin registry
- **Benefit:** Extensibility without core changes

### 6. Policy Trait System
- **Impact:** Better maintainability, eliminates duplication
- **Effort:** 2-3 weeks
- **Approach:** Single evaluation loop with trait-based policies
- **Benefit:** Easier to add new policies, testable logic

### 7. Dependency Injection for Testability
- **Impact:** Better testability, minimal runtime impact
- **Effort:** 4-6 weeks
- **Approach:** Trait-based abstractions for I/O and git operations
- **Benefit:** Unit tests without real filesystem/git

### 8. Streaming Output for Large Repos
- **Impact:** Constant memory usage, handles very large repos
- **Effort:** 2-3 weeks
- **Approach:** Stream results as computed, incremental JSON/HTML
- **Benefit:** Enables analysis of repos with 100k+ functions

---

## Lower Priority: Quality of Life

### 9. Language Plugin System
- **Impact:** Enables community language support
- **Effort:** 6-8 weeks
- **Approach:** Dynamic language registration with plugin API
- **Benefit:** No core changes for new languages

### 10. Structured Error Types
- **Impact:** Better error handling and debugging
- **Effort:** 2-3 weeks
- **Approach:** Domain-specific error types instead of `anyhow`
- **Benefit:** Programmatic error handling and recovery

### 11. AST Storage Optimization
- **Impact:** 30-50% memory reduction
- **Effort:** 2-3 weeks
- **Approach:** Lazy AST parsing, compact representations
- **Benefit:** Better for large repos

### 12. Configuration Validation & Schema
- **Impact:** Better developer experience
- **Effort:** 1-2 weeks
- **Approach:** JSON Schema for config, validation with clear errors
- **Benefit:** Catch errors early, IDE autocomplete

---

## Implementation Roadmap

### Phase 1: Performance (3-6 months)
**Focus:** Speed and scalability
1. Parallel file analysis (2-3 weeks)
2. Batched git operations (1-2 weeks)
3. Incremental analysis (4-6 weeks)

**Expected Outcome:** 10-100x faster for incremental workflows

### Phase 2: Architecture (6-9 months)
**Focus:** Maintainability and testability
4. Policy trait system (2-3 weeks)
5. Dependency injection (4-6 weeks)
6. Streaming output (2-3 weeks)

**Expected Outcome:** Reduced duplication, better testability

### Phase 3: Extensibility (9-12 months)
**Focus:** Plugin systems and extensibility
7. Metric plugin system (3-4 weeks)
8. Language plugin system (6-8 weeks)
9. Structured errors (2-3 weeks)

**Expected Outcome:** Community extensibility, better error handling

---

## Key Trade-offs

### Performance vs. Simplicity
- **Decision:** Start with parallelization, add caching later
- **Rationale:** Parallelization provides immediate benefit with manageable complexity

### Extensibility vs. Performance
- **Decision:** Use generics where possible, traits where necessary
- **Rationale:** Balance between zero-cost abstractions and runtime flexibility

### Memory vs. Speed
- **Decision:** Make configurable, default to balanced
- **Rationale:** Different repos have different constraints

### Backward Compatibility
- **Decision:** Version APIs, provide migration guides
- **Rationale:** Most improvements can be additive, some require breaking changes

---

## Success Metrics

### Performance Targets
- **5-10x faster** for large repos (>10k functions)
- **10-100x faster** for incremental analysis
- **Constant memory** usage for streaming output

### Maintainability Targets
- **50% reduction** in code duplication
- **<100 lines** to add new metric/language
- **100% test coverage** for core components

### Scalability Targets
- Handle repos with **100k+ functions**
- Support **10+ languages** via plugins
- **Sub-second** analysis for incremental changes

---

## Risk Assessment

### Low Risk
- Parallel file analysis (proven pattern, `rayon` is mature)
- Batched git operations (straightforward optimization)
- Configuration validation (additive feature)

### Medium Risk
- Incremental analysis (cache invalidation complexity)
- Policy trait system (migration effort)
- Dependency injection (large refactoring)

### High Risk
- Call graph optimizations (algorithm correctness)
- Language plugin system (API design complexity)
- Streaming output (HTML streaming complexity)

---

## Recommendations

### Immediate Actions (Next 3 Months)
1. **Implement parallel file analysis** — Highest ROI, low risk
2. **Batch git operations** — Quick win, significant speedup
3. **Add configuration validation** — Improves DX with minimal effort

### Short-term (3-6 Months)
4. **Incremental analysis** — Enables faster CI feedback
5. **Policy trait system** — Reduces maintenance burden
6. **Dependency injection** — Improves testability

### Long-term (6-12 Months)
7. **Plugin systems** — Enables community contributions
8. **Streaming output** — Handles very large repos
9. **Error type improvements** — Better error handling

---

## Conclusion

The proposed improvements would transform Hotspots from a fast single-threaded tool into a highly scalable, extensible platform. The Phase 1 performance improvements alone could provide 10-100x speedup for typical workflows, making Hotspots viable for very large codebases and faster CI integration.

The architectural improvements in Phase 2 would reduce maintenance burden and improve testability, while Phase 3's plugin systems would enable community contributions and long-term extensibility.

**Priority:** Focus on Phase 1 performance improvements first, as they provide the highest immediate value with manageable risk.

---

**Full Details:** See [IMPROVEMENTS.md](./IMPROVEMENTS.md) for complete specifications, code examples, and implementation details.
