# CI/CD & GitHub Action

Integrate Hotspots into your CI/CD pipeline to catch complexity regressions before they reach production.

## GitHub Action (Recommended)

### Quick Start

Create `.github/workflows/hotspots.yml`:

```yaml
name: Hotspots

on:
  pull_request:
  push:
    branches: [main]

jobs:
  analyze:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: write  # For PR comments

    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # Required for delta analysis

      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

**That's it!** The action will:
- Analyze your code on every PR
- Post results as PR comments
- Generate HTML reports
- Fail builds on policy violations

---

## Action Inputs

### Required

| Input | Description | Default |
|-------|-------------|---------|
| `github-token` | GitHub token for posting PR comments | `github.token` (auto) |

### Optional

| Input | Description | Default |
|-------|-------------|---------|
| `path` | Path to analyze | `.` (repo root) |
| `policy` | Policy to enforce | `critical-introduction` |
| `min-lrs` | Minimum LRS threshold (overrides policy) | - |
| `config` | Path to config file | Auto-discover |
| `fail-on` | When to fail: `error`, `warn`, `never` | `error` |
| `version` | Hotspots version to use | `latest` |
| `post-comment` | Post PR comment | `true` |

**Available policies:** `critical-introduction` (default), `strict`, `moderate`, `custom`

---

## Action Outputs

| Output | Type | Description |
|--------|------|-------------|
| `violations` | JSON array | Policy violations |
| `passed` | boolean | Whether analysis passed |
| `summary` | string | Markdown summary |
| `report-path` | string | Path to HTML report |
| `json-output` | string | Path to JSON output |

---

## How It Works

### PR Context (Delta Mode)

When run on a pull request:
1. Detects merge-base automatically
2. Analyzes only modified functions
3. Compares complexity before vs. after
4. Evaluates policies and checks for violations
5. Posts results as a PR comment (updates on each push)

### Push Context (Snapshot Mode)

When run on the main branch:
1. Analyzes entire codebase
2. Creates a snapshot (stored in `.hotspots/snapshots/`)
3. Reports violations in job summary
4. Snapshot used as baseline for future PRs

---

## Workflow Examples

### Basic PR Check

```yaml
name: Complexity Check

on: [pull_request]

jobs:
  hotspots:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: write

    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

### With Custom Config

```yaml
- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    config: .hotspots.ci.json
    github-token: ${{ secrets.GITHUB_TOKEN }}
```

`.hotspots.ci.json`:
```json
{
  "exclude": ["**/*.test.ts", "**/__tests__/**"],
  "min_lrs": 6.0,
  "thresholds": {
    "moderate": 5.0,
    "high": 8.0,
    "critical": 10.0
  }
}
```

### Monorepo Setup

```yaml
jobs:
  frontend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          path: packages/frontend
          github-token: ${{ secrets.GITHUB_TOKEN }}

  backend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          path: packages/backend
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

### Upload HTML Report as Artifact

```yaml
- uses: Stephen-Collins-tech/hotspots-action@v1
  id: hotspots
  with:
    github-token: ${{ secrets.GITHUB_TOKEN }}

- uses: actions/upload-artifact@v4
  if: always()
  with:
    name: hotspots-report-${{ github.sha }}
    path: ${{ steps.hotspots.outputs.report-path }}
    retention-days: 30
```

### Warning Mode (Don't Fail Builds)

```yaml
- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    fail-on: never
    github-token: ${{ secrets.GITHUB_TOKEN }}
```

### Scheduled Analysis

```yaml
name: Weekly Complexity Report

on:
  schedule:
    - cron: '0 0 * * 0'  # Every Sunday at midnight

jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          fail-on: never
      - uses: actions/upload-artifact@v4
        with:
          name: weekly-complexity-report
          path: .hotspots/report.html
```

---

## Permissions

```yaml
permissions:
  contents: read        # Checkout code
  pull-requests: write  # Post PR comments
```

If you don't want PR comments, use `post-comment: false` and only `contents: read`.

---

## Troubleshooting (GitHub Actions)

### "failed to extract git context"

Use `fetch-depth: 0` in checkout:
```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0
```

### "merge-base not found"

Fetch the base branch explicitly:
```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0
    ref: ${{ github.event.pull_request.head.ref }}

- name: Fetch base branch
  run: git fetch origin ${{ github.event.pull_request.base.ref }}
```

### PR Comments Not Posting

Ensure `pull-requests: write` permission and `github-token` is provided.

---

## Other CI Systems

### GitLab CI

```yaml
stages:
  - analyze

hotspots:
  stage: analyze
  image: rust:latest
  before_script:
    - cargo install hotspots-cli
  script:
    - hotspots analyze src/ --mode delta --policy --format json
  artifacts:
    paths:
      - .hotspots/report.html
    expire_in: 1 week
  rules:
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'
```

**Tip:** Use a pre-built Docker image for faster startup:
```yaml
  image: ghcr.io/stephen-collins-tech/hotspots:latest
```

### CircleCI

```yaml
version: 2.1

jobs:
  hotspots:
    docker:
      - image: rust:latest
    steps:
      - checkout
      - restore_cache:
          keys:
            - cargo-v1-{{ checksum "Cargo.lock" }}
      - run:
          name: Install Hotspots
          command: |
            if ! command -v hotspots &> /dev/null; then
              cargo install hotspots-cli
            fi
      - save_cache:
          key: cargo-v1-{{ checksum "Cargo.lock" }}
          paths:
            - ~/.cargo
      - run:
          name: Run Analysis
          command: hotspots analyze src/ --mode delta --policy
      - store_artifacts:
          path: .hotspots/report.html

workflows:
  analyze:
    jobs:
      - hotspots
```

### Travis CI

```yaml
language: rust
rust:
  - stable

install:
  - cargo install hotspots-cli

script:
  - hotspots analyze src/ --mode delta --policy --format json
```

### Jenkins

```groovy
pipeline {
    agent any

    stages {
        stage('Setup') {
            steps {
                sh 'cargo install hotspots-cli || true'
            }
        }

        stage('Analyze') {
            steps {
                sh 'hotspots analyze src/ --mode delta --policy --format json > hotspots-output.json'
            }
        }

        stage('Report') {
            steps {
                archiveArtifacts artifacts: '.hotspots/report.html', fingerprint: true
                publishHTML([
                    reportDir: '.hotspots',
                    reportFiles: 'report.html',
                    reportName: 'Hotspots Report'
                ])
            }
        }
    }

    post {
        failure {
            echo 'Hotspots found blocking violations'
        }
    }
}
```

### Bitbucket Pipelines

```yaml
pipelines:
  pull-requests:
    '**':
      - step:
          name: Hotspots PR Check
          image: rust:latest
          caches:
            - cargo
          script:
            - cargo install hotspots-cli
            - hotspots analyze src/ --mode delta --policy
          artifacts:
            - .hotspots/report.html
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success (no violations, or only warnings) |
| `1` | Failure (blocking policy violations) |

Control exit behavior:
```bash
# Never fail (reporting only)
hotspots analyze src/ --mode delta --policy --fail-on never

# Ignore exit code in CI script
hotspots analyze src/ --mode delta --policy || true
```

---

## Environment Variables for PR Detection

When the following env vars are set, delta mode compares against the merge-base instead of direct parent:

- **GitHub Actions:** `GITHUB_EVENT_NAME=pull_request`
- **GitLab CI:** `CI_MERGE_REQUEST_IID`
- **CircleCI:** `CIRCLE_PULL_REQUEST`
- **Travis CI:** `TRAVIS_PULL_REQUEST`

---

## Best Practices

1. **Use delta mode for PRs** — Only shows what changed, fast and focused
2. **Persist snapshots on main** — Creates baselines for future PRs
3. **Start with `fail-on: never`** — Observe without blocking, then tighten
4. **Archive HTML reports** — Keep 30-day history for trend review
5. **Separate dev and CI configs** — Stricter thresholds in CI, lenient locally
