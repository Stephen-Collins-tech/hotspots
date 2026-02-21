# Project Status

_Last updated: 2026-02-21_

---

## Release: v1.0.0 ✅

Tagged and pushed to `main`. All 233 tests passing.

**What shipped:**
- 6-language complexity analysis (TypeScript, JavaScript, Go, Python, Rust, Java)
- CC / ND / FO / NS metrics → LRS composite risk score
- Snapshot + delta + policy modes
- Call graph with PageRank / SCC / betweenness centrality
- Driver labels + quadrant classification (`fire` / `debt` / `watch` / `ok`)
- Explain mode (`--explain`)
- File + module level views (`--level file`, `--level module`)
- HTML reports with trend charts
- Agent-optimized JSON output (Schema v3 with quadrant buckets)
- JSONL output, `--all-functions`, `--no-persist`, `--per-function-touches`
- GitHub Action (8 inputs, 4 outputs, PR delta + push snapshot modes)
- Policy engine (critical-introduction, excessive-regression, net-repo-regression)
- Suppression comments (`// hotspots-ignore: <reason>`)
- `hotspots config show` / `hotspots config validate`
- Per-function touch cache (warm runs fast, cold ~6s)
- Docs site restructured: 27 pages → 11, flat sidebar

**Known gaps (non-blocking):**
- `compact` levels 1 and 2 not yet implemented (errors gracefully)
- Java/Python CFG edge cases (4 in-code TODOs: ternary, boolean ops, match)
- MCP server: planned, documented as "coming soon"

---

## Repository: Still Private

Repo is not yet public. Make public when ready to announce.

---

## Pending: Docs Deployment

Docs site (VitePress, `/docs`) to deploy at `docs.hotspots.dev`.

**Decision: Cloudflare Pages** (fits existing Cloudflare/hotspots.dev setup)

**Cloudflare Pages settings:**
```
Project name:  hotspots-docs
Repository:    Stephen-Collins-tech/hotspots
Branch:        main
Build command: cd docs && npx vitepress build
Output dir:    docs/.vitepress/dist
```

**DNS (Cloudflare):** Add CNAME `docs` → `hotspots-docs.pages.dev`

_Not yet set up — needs to be done in Cloudflare dashboard._

---

## Pending: `hotspots-cloud` Repo (Private)

A private sibling repo for the commercial product. Everything that is not open source lives here.

### Architecture

| URL | Description |
|-----|-------------|
| `hotspots.dev` | Marketing landing page + leaderboard |
| `docs.hotspots.dev` | Open-source docs (from this repo) |
| `hotspots.dev/api/*` | Cloudflare Workers (leaderboard API, crawl) |
| `app.hotspots.dev` *(future)* | Premium product dashboard |

**Infra stack:** Cloudflare Pages + Workers + D1 (SQLite)

---

### `hotspots.dev` — Marketing Landing Page

Sections (in order):
1. Hero — tagline, terminal demo, primary CTA(s)
2. Leaderboard preview — top 10 repos, link to full leaderboard
3. How it works — 3-step visual
4. Supported languages — badge row
5. CTAs — _TBD (candidates: install, star, docs, try on your repo)_
6. Footer — docs link, GitHub, license

---

### Leaderboard

Auto-crawled public GitHub repos ranked by complexity risk.

**Default sort:** critical function count (LRS ≥ 9.0) descending
**Filterable by:** language

**Columns:**
- Rank
- Repository (`owner/name`)
- Language
- Critical functions (LRS ≥ 9.0)
- High functions (LRS ≥ 6.0)
- Avg LRS
- Max LRS
- Last analyzed

**Crawl pipeline:**
- Seed: GitHub "most starred" repos per language (~500–1000 to start)
- Schedule: weekly cron (Cloudflare Worker Cron Trigger)
- Process: `git clone --depth 1` → `hotspots analyze --format json --all-functions` → upsert D1 → delete clone

**D1 schema (sketch):**
```sql
repos      (id, owner, name, language, stars, last_analyzed_at)
snapshots  (id, repo_id, analyzed_at, avg_lrs, max_lrs, critical_count, high_count, function_count)
```

**API (Worker):**
```
GET /api/leaderboard?lang=&sort=&limit=&offset=
GET /api/repo/:owner/:name
```

---

### Premium Features (Private, `hotspots-cloud`)

TBD — placeholder candidates:
- Hosted dashboard (analyze without running CLI locally)
- GitHub App (auto-analysis on PRs, no `action.yml` required)
- Team / org accounts with shared snapshot history
- Historical trending beyond local `.hotspots/` snapshots
- Private repo analysis via GitHub OAuth
- API access (serve analysis results programmatically)
- Alerting (Slack, email, webhook)

---

### Phase 1 Scope (`hotspots-cloud`)

- [ ] Set up repo + Cloudflare Pages deployment
- [ ] Landing page (hero + leaderboard preview + language badges + footer)
- [ ] Full leaderboard page (sort, filter by language)
- [ ] Crawl Worker — seed ~100 repos, weekly cron
- [ ] D1 schema + leaderboard API endpoints
- [ ] Link to `docs.hotspots.dev` in nav/footer
- [ ] Finalize CTAs
- [ ] Custom domain: `hotspots.dev` → Cloudflare Pages

**Out of scope for phase 1:** per-repo detail pages, trend history charts, user submissions, auth, billing, premium features

---

## Next Actions (in order)

1. **Set repo public** — when ready to announce
2. **Deploy docs** — Cloudflare Pages dashboard setup for `docs.hotspots.dev`
3. **Create `hotspots-cloud` repo** — private, scaffold phase 1
4. **Define CTAs** — decide what the primary CTA on `hotspots.dev` is
5. **Define premium features** — flesh out what paid tier looks like
6. **MCP server** — design + implement (currently "coming soon")
