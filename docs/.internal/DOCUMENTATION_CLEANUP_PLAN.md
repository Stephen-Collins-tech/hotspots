# Documentation Cleanup & Reorganization Plan (REVISED)

**Date:** 2026-02-08
**Status:** IN PROGRESS
**Target:** Documentation site at `docs.hotspots.dev` + developer repo usage

---

## Executive Summary

**Problem:** 50+ documentation files scattered across root and docs/ with heavy duplication

**Goal:** Create clean documentation structure that:
1. ✅ Powers documentation website at `docs.hotspots.dev`
2. ✅ Works for developers cloning the repo
3. ✅ Eliminates all duplication
4. ✅ Has clear information hierarchy

**Approach:** Structure docs/ to be directly usable by static site generators (VitePress, Docusaurus, etc.) while remaining readable in GitHub and local clones.

---

## Proposed Structure (Docs Site Ready)

```
hotspots/
├── README.md                          # Project overview (links to docs/)
├── CHANGELOG.md                       # Release history
├── CLAUDE.md                          # AI coding conventions
├── LICENSE                            # MIT license
├── CONTRIBUTING.md                    # Quick contributor guide (links to docs/contributing/)
│
└── docs/                              # ALL documentation (powers docs.hotspots.dev)
    ├── index.md                       # Docs landing page (replaces README.md)
    │
    ├── getting-started/               # Installation & quickstart
    │   ├── installation.md
    │   ├── quick-start.md
    │   └── quick-start-react.md
    │
    ├── guide/                         # User guide (not "user-guide")
    │   ├── usage.md                   # CLI reference
    │   ├── configuration.md           # Config files
    │   ├── ci-integration.md          # CI/CD setup
    │   ├── github-action.md           # GitHub Actions
    │   ├── suppression.md             # Suppression comments
    │   └── output-formats.md          # JSON, HTML, text
    │
    ├── reference/                     # API & technical specs
    │   ├── metrics.md                 # How metrics are calculated
    │   ├── lrs-spec.md               # LRS formula
    │   ├── cli.md                    # Complete CLI reference
    │   ├── json-schema.md            # Output schemas
    │   ├── language-support.md       # Supported languages
    │   └── limitations.md            # Known limitations
    │
    ├── architecture/                  # For contributors
    │   ├── overview.md
    │   ├── design-decisions.md
    │   ├── invariants.md
    │   ├── multi-language.md
    │   └── testing.md
    │
    ├── contributing/                  # Contributor docs
    │   ├── index.md                  # Main contributing guide
    │   ├── development.md            # Dev setup
    │   ├── adding-languages.md       # Language support
    │   └── releases.md               # Release process
    │
    ├── integrations/                  # AI & tooling integrations
    │   ├── mcp-server.md             # Model Context Protocol
    │   ├── ai-agents.md              # AI agent examples
    │   └── api.md                    # Programmatic API (if we build one)
    │
    └── .internal/                     # NOT for docs site (git-ignored on site)
        ├── roadmap.md
        ├── tasks.md
        ├── session-handoffs/
        └── archive/
```

---

## Key Differences from Original Plan

### ❌ Removed
- Multiple README.md files in subdirectories (docs site doesn't need them)
- `ai-integration/` renamed to `integrations/` (more general)
- `user-guide/` renamed to `guide/` (shorter, conventional)
- `research/` folder (move to `.internal/`)

### ✅ Added
- `docs/index.md` - Main docs landing page (not README.md)
- `CONTRIBUTING.md` in root - Links to docs/contributing/
- `docs/reference/cli.md` - Complete CLI reference
- `.internal/` prefix to hide from docs site

### 🎯 Optimized For
- **VitePress/Docusaurus**: Direct markdown → HTML
- **GitHub**: Readable without site generator
- **Local dev**: Clear hierarchy, easy navigation
- **Docs site**: Clean URLs (e.g., `/guide/usage`, not `/user-guide/usage`)

---

## Documentation Site Configuration

### Example VitePress Config (`.vitepress/config.js`)

```javascript
export default {
  title: 'Hotspots',
  description: 'Multi-language complexity analysis for high-leverage refactoring',

  themeConfig: {
    nav: [
      { text: 'Guide', link: '/guide/usage' },
      { text: 'Reference', link: '/reference/metrics' },
      { text: 'GitHub', link: 'https://github.com/Stephen-Collins-tech/hotspots' }
    ],

    sidebar: {
      '/getting-started/': [
        {
          text: 'Getting Started',
          items: [
            { text: 'Installation', link: '/getting-started/installation' },
            { text: 'Quick Start', link: '/getting-started/quick-start' },
            { text: 'React Projects', link: '/getting-started/quick-start-react' }
          ]
        }
      ],

      '/guide/': [
        {
          text: 'User Guide',
          items: [
            { text: 'CLI Usage', link: '/guide/usage' },
            { text: 'Configuration', link: '/guide/configuration' },
            { text: 'CI Integration', link: '/guide/ci-integration' },
            { text: 'GitHub Action', link: '/guide/github-action' },
            { text: 'Suppression', link: '/guide/suppression' },
            { text: 'Output Formats', link: '/guide/output-formats' }
          ]
        }
      ],

      '/reference/': [
        {
          text: 'Reference',
          items: [
            { text: 'Metrics', link: '/reference/metrics' },
            { text: 'LRS Specification', link: '/reference/lrs-spec' },
            { text: 'CLI Reference', link: '/reference/cli' },
            { text: 'JSON Schema', link: '/reference/json-schema' },
            { text: 'Language Support', link: '/reference/language-support' },
            { text: 'Limitations', link: '/reference/limitations' }
          ]
        }
      ],

      '/architecture/': [
        {
          text: 'Architecture',
          items: [
            { text: 'Overview', link: '/architecture/overview' },
            { text: 'Design Decisions', link: '/architecture/design-decisions' },
            { text: 'Invariants', link: '/architecture/invariants' },
            { text: 'Multi-Language', link: '/architecture/multi-language' },
            { text: 'Testing', link: '/architecture/testing' }
          ]
        }
      ],

      '/contributing/': [
        {
          text: 'Contributing',
          items: [
            { text: 'Getting Started', link: '/contributing/' },
            { text: 'Development', link: '/contributing/development' },
            { text: 'Adding Languages', link: '/contributing/adding-languages' },
            { text: 'Releases', link: '/contributing/releases' }
          ]
        }
      ]
    }
  }
}
```

---

## File Mapping (REVISED)

### Keep in Root (5 files max)
```
README.md              → Keep (project overview, links to docs/)
CHANGELOG.md           → Keep (release history)
CLAUDE.md              → Keep (AI conventions)
LICENSE                → Keep
CONTRIBUTING.md        → Create new (brief, links to docs/contributing/)
```

### Move to docs/ Structure

#### Getting Started
```
QUICK_START_REACT.md                    → docs/getting-started/quick-start-react.md
[NEW]                                   → docs/getting-started/installation.md
[NEW]                                   → docs/getting-started/quick-start.md
```

#### Guide
```
docs/USAGE.md                           → docs/guide/usage.md
docs/suppression.md                     → docs/guide/suppression.md
[NEW]                                   → docs/guide/configuration.md
[NEW]                                   → docs/guide/ci-integration.md
[NEW]                                   → docs/guide/github-action.md
[NEW]                                   → docs/guide/output-formats.md
```

#### Reference
```
docs/lrs-spec.md                        → docs/reference/lrs-spec.md
docs/json-schema.md                     → docs/reference/json-schema.md
docs/metrics-calculation-and-rationale.md → docs/reference/metrics.md
docs/language-support.md                → docs/reference/language-support.md
docs/limitations.md                     → docs/reference/limitations.md
[NEW]                                   → docs/reference/cli.md
```

#### Architecture
```
docs/architecture.md                    → docs/architecture/overview.md
docs/design-decisions.md                → docs/architecture/design-decisions.md
docs/invariants.md                      → docs/architecture/invariants.md
MULTI_LANGUAGE_ANALYSIS.md              → docs/architecture/multi-language.md (extract relevant content)
[NEW]                                   → docs/architecture/testing.md
```

#### Contributing
```
RELEASE_PROCESS.md                      → docs/contributing/releases.md
docs/VERSIONING.md                      → Merge into docs/contributing/releases.md
docs/LIVE_TESTING_GUIDE.md              → Merge into docs/contributing/development.md
[NEW]                                   → docs/contributing/index.md
[NEW]                                   → docs/contributing/development.md
[NEW]                                   → docs/contributing/adding-languages.md
```

#### Integrations
```
docs/AI_INTEGRATION.md                  → docs/integrations/mcp-server.md (refocus on MCP)
packages/mcp-server/README.md           → Copy content to docs/integrations/mcp-server.md
examples/ai-agents/README.md            → Copy content to docs/integrations/ai-agents.md
```

#### Internal (Hidden from Docs Site)
```
ROADMAP.md + ROADMAP_SUMMARY.md + docs/roadmap.md → docs/.internal/roadmap.md (consolidate)
TASKS.md                                → docs/.internal/tasks.md
HANDOFF.md                              → docs/.internal/session-handoffs/handoff-old.md
docs/session-handoff-*.md               → docs/.internal/session-handoffs/

# Archive (historical)
CODEBASE_REVIEW.txt                     → docs/.internal/archive/
GITHUB_ACTION_SETUP_COMPLETE.md         → docs/.internal/archive/
IMPLEMENTATION_SUMMARY.md               → docs/.internal/archive/
IMPROVEMENTS_REPORT.md                  → docs/.internal/archive/
JSX_TSX_IMPLEMENTATION.md               → docs/.internal/archive/
MULTI_LANGUAGE_PLAN.md                  → docs/.internal/archive/
PROGRESS.md                             → docs/.internal/archive/
PROJECT_STATUS.md                       → docs/.internal/archive/
RELEASE.md                              → docs/.internal/archive/
docs/FEATURE-SUMMARY.md                 → docs/.internal/archive/
docs/GIT_HISTORY_INTEGRATION_TASKS.md   → docs/.internal/archive/
docs/PROJECT_STATE.md                   → docs/.internal/archive/
docs/STAGED_FEATURES_REPORT.md          → docs/.internal/archive/
docs/TEST_RESULTS.md                    → docs/.internal/archive/
docs/TASKS.md                           → docs/.internal/archive/
docs/git-history-integration-summary.md → docs/.internal/archive/
docs/implementation-summary.md          → docs/.internal/archive/
docs/mvp-implementation-history.md      → docs/.internal/archive/
docs/test-summary.md                    → docs/.internal/archive/
docs/future-history-command.md          → docs/.internal/archive/
docs/synthetic-harness-research.md      → docs/.internal/archive/
docs/capabilities-and-use-cases.md      → Merge into docs/index.md or delete
```

---

## Implementation Phases (REVISED)

### Phase 1: Create Structure ✅ (DONE)
- [x] Create directory structure
- [x] Create placeholder READMEs (will replace with index.md)

### Phase 2: Create Core Files (30 min)
- [ ] Create `docs/index.md` (main landing page)
- [ ] Create `CONTRIBUTING.md` in root
- [ ] Create `.gitignore` entry for `docs/.internal/` on docs site

### Phase 3: Move & Rename Core Docs (1 hour)
- [ ] Move getting-started docs
- [ ] Move guide docs (usage, suppression, etc.)
- [ ] Move reference docs (metrics, lrs-spec, etc.)
- [ ] Move architecture docs
- [ ] Move contributing docs

### Phase 4: Consolidate Duplicates (1 hour)
- [ ] Merge 3 ROADMAP files → `docs/.internal/roadmap.md`
- [ ] Move TASKS.md → `docs/.internal/tasks.md`
- [ ] Merge RELEASE + VERSIONING → `docs/contributing/releases.md`

### Phase 5: Archive Historical (30 min)
- [ ] Move all historical docs to `docs/.internal/archive/`
- [ ] Add timestamps to filenames
- [ ] Create archive index

### Phase 6: Create Missing Docs (3-4 hours)
- [ ] `docs/getting-started/installation.md`
- [ ] `docs/getting-started/quick-start.md`
- [ ] `docs/guide/configuration.md`
- [ ] `docs/guide/ci-integration.md`
- [ ] `docs/guide/github-action.md`
- [ ] `docs/guide/output-formats.md`
- [ ] `docs/reference/cli.md`
- [ ] `docs/contributing/index.md`
- [ ] `docs/contributing/development.md`
- [ ] `docs/contributing/adding-languages.md`
- [ ] `docs/architecture/multi-language.md`
- [ ] `docs/architecture/testing.md`

### Phase 7: Update Cross-References (1 hour)
- [ ] Update all internal doc links
- [ ] Update root README.md to link to docs/
- [ ] Update CONTRIBUTING.md to link to docs/contributing/
- [ ] Verify all links work

### Phase 8: Cleanup (30 min)
- [ ] Delete duplicate files from root
- [ ] Delete old README.md files in subdirectories
- [ ] Verify no broken links
- [ ] Test locally

### Phase 9: Docs Site Setup (2 hours) - FUTURE
- [ ] Choose framework (VitePress recommended)
- [ ] Add `.vitepress/` or `.docusaurus/` config
- [ ] Configure sidebar/navigation
- [ ] Set up deployment (Vercel/Netlify)
- [ ] Configure custom domain (docs.hotspots.dev)

---

## Success Metrics

- ✅ Root directory has ≤5 documentation files
- ✅ All docs in `docs/` directory
- ✅ No duplicate content (3 ROADMAPs → 1, etc.)
- ✅ Clear hierarchy (getting-started → guide → reference → architecture)
- ✅ Works in GitHub (readable markdown)
- ✅ Works for docs site (clean URLs, no README.md clutter)
- ✅ Internal docs separated (`.internal/` hidden from site)
- ✅ Zero broken links

---

## Docs Site Deployment (Future)

### Recommended: VitePress
```bash
npm install -D vitepress
npx vitepress init
# Configure docs/ as source directory
# Deploy to Vercel with custom domain
```

### Alternative: Docusaurus
```bash
npx create-docusaurus@latest docs-site classic
# Move docs/ content to docs-site/docs/
```

### DNS Configuration
```
docs.hotspots.dev → CNAME → hotspots-docs.vercel.app
```

---

## Next Steps

1. ✅ Review this revised plan
2. ⏳ Execute Phase 2: Create core files
3. ⏳ Execute Phase 3: Move core docs
4. ⏳ Continue through Phase 8

**Total Estimated Time:** 8-10 hours (down from 10-15)

---

**End of Revised Plan**
