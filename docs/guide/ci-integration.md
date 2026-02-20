# CI/CD Integration

Integrate Hotspots into your CI/CD pipeline to catch complexity regressions before they reach production.

## Overview

Hotspots provides first-class CI/CD support:
- **Zero-config GitHub Action** for seamless integration
- **Policy enforcement** to block risky changes
- **Delta analysis** comparing changes vs. baseline
- **PR comments** with actionable insights
- **HTML reports** as workflow artifacts
- **Exit codes** for build failure control

**Supported CI Systems:** GitHub Actions, GitLab CI, CircleCI, Travis CI, Jenkins, Bitbucket Pipelines

---

## GitHub Actions (Recommended)

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
- Detect PRs automatically and run delta analysis
- Post results as PR comments
- Generate HTML reports as artifacts
- Fail the build on policy violations

See [GitHub Action Guide](./github-action.md) for complete documentation.

### Action Inputs

| Input | Description | Default |
|-------|-------------|---------|
| `path` | Path to analyze | `.` (repo root) |
| `policy` | Policy mode | `critical-introduction` |
| `min-lrs` | Min LRS threshold | - |
| `config` | Path to config file | Auto-discover |
| `fail-on` | When to fail: `error`, `warn`, `never` | `error` |
| `version` | Hotspots version | `latest` |
| `github-token` | Token for PR comments | `github.token` (auto) |
| `post-comment` | Post PR comment | `true` |

### Action Outputs

| Output | Description |
|--------|-------------|
| `violations` | JSON array of violations |
| `passed` | `true`/`false` |
| `summary` | Markdown summary |
| `report-path` | HTML report path |
| `json-output` | JSON output path |

### Example Configurations

#### Strict Policy (No Regressions)

```yaml
- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    policy: strict
    fail-on: error
    github-token: ${{ secrets.GITHUB_TOKEN }}
```

#### Custom Threshold

```yaml
- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    min-lrs: 8.0  # Only flag functions with LRS ≥ 8
    fail-on: warn
    github-token: ${{ secrets.GITHUB_TOKEN }}
```

#### With Configuration File

```yaml
- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    config: .hotspots.ci.json
    github-token: ${{ secrets.GITHUB_TOKEN }}
```

Create `.hotspots.ci.json`:
```json
{
  "exclude": ["**/*.test.ts", "**/__tests__/**"],
  "min_lrs": 5.0,
  "thresholds": {
    "moderate": 5.0,
    "high": 8.0,
    "critical": 10.0
  }
}
```

#### Multi-Path Analysis

```yaml
- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    path: src/
    github-token: ${{ secrets.GITHUB_TOKEN }}

- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    path: lib/
    github-token: ${{ secrets.GITHUB_TOKEN }}
```

#### Upload HTML Report

```yaml
- uses: Stephen-Collins-tech/hotspots-action@v1
  id: hotspots
  with:
    github-token: ${{ secrets.GITHUB_TOKEN }}

- uses: actions/upload-artifact@v4
  if: always()
  with:
    name: hotspots-report
    path: ${{ steps.hotspots.outputs.report-path }}
    retention-days: 30
```

#### Use Outputs in Subsequent Steps

```yaml
- uses: Stephen-Collins-tech/hotspots-action@v1
  id: hotspots
  with:
    fail-on: never  # Don't fail, handle manually
    github-token: ${{ secrets.GITHUB_TOKEN }}

- name: Check Results
  run: |
    if [ "${{ steps.hotspots.outputs.passed }}" == "false" ]; then
      echo "::warning::Hotspots found violations"
      echo "${{ steps.hotspots.outputs.summary }}"
    fi
```

---

## GitLab CI

### Installation

Add to `.gitlab-ci.yml`:

```yaml
stages:
  - analyze

hotspots:
  stage: analyze
  image: rust:latest
  before_script:
    - cargo install hotspots-cli --version 1.0.0
  script:
    - hotspots analyze src/ --mode delta --policy --format json
  artifacts:
    reports:
      junit: hotspots-report.xml
    paths:
      - .hotspots/report.html
    expire_in: 1 week
  rules:
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'
```

### With Configuration File

```yaml
hotspots:
  stage: analyze
  image: rust:latest
  before_script:
    - cargo install hotspots-cli
  script:
    - hotspots analyze src/ --config .hotspots.ci.json --mode delta --policy
  artifacts:
    paths:
      - .hotspots/report.html
  only:
    - merge_requests
```

### Docker Image (Faster)

```yaml
hotspots:
  stage: analyze
  image: ghcr.io/stephen-collins-tech/hotspots:latest
  script:
    - hotspots analyze src/ --mode delta --policy --format json
  artifacts:
    paths:
      - .hotspots/report.html
```

---

## CircleCI

### Configuration

Add to `.circleci/config.yml`:

```yaml
version: 2.1

jobs:
  hotspots:
    docker:
      - image: rust:latest
    steps:
      - checkout
      - run:
          name: Install Hotspots
          command: cargo install hotspots-cli
      - run:
          name: Run Analysis
          command: hotspots analyze src/ --mode delta --policy --format json
      - store_artifacts:
          path: .hotspots/report.html
          destination: hotspots-report

workflows:
  analyze:
    jobs:
      - hotspots:
          filters:
            branches:
              only: main
```

### With Caching

```yaml
jobs:
  hotspots:
    docker:
      - image: rust:latest
    steps:
      - checkout
      - restore_cache:
          keys:
            - cargo-v1-{{ checksum "Cargo.lock" }}
            - cargo-v1-
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
```

---

## Travis CI

### Configuration

Add to `.travis.yml`:

```yaml
language: rust
rust:
  - stable

install:
  - cargo install hotspots-cli

script:
  - hotspots analyze src/ --mode delta --policy --format json

after_success:
  - echo "Hotspots analysis passed"
```

### PR-Only Analysis

```yaml
jobs:
  include:
    - stage: analyze
      if: type = pull_request
      script:
        - hotspots analyze src/ --mode delta --policy
```

---

## Jenkins

### Pipeline Script

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

### Freestyle Project

**Build Steps:**
```bash
# Install Hotspots (first time)
cargo install hotspots-cli

# Run analysis
hotspots analyze src/ --mode delta --policy --format json
```

**Post-build Actions:**
- Archive artifacts: `.hotspots/report.html`
- Publish HTML reports: `.hotspots/report.html`

---

## Bitbucket Pipelines

### Configuration

Add to `bitbucket-pipelines.yml`:

```yaml
pipelines:
  default:
    - step:
        name: Hotspots Analysis
        image: rust:latest
        caches:
          - cargo
        script:
          - cargo install hotspots-cli
          - hotspots analyze src/ --mode delta --policy --format json
        artifacts:
          - .hotspots/report.html

  pull-requests:
    '**':
      - step:
          name: Hotspots PR Check
          image: rust:latest
          script:
            - cargo install hotspots-cli
            - hotspots analyze src/ --mode delta --policy
```

---

## Docker Integration

### Using Pre-built Image

```bash
docker run --rm -v $(pwd):/workspace ghcr.io/stephen-collins-tech/hotspots:latest \
  analyze /workspace/src --mode delta --policy --format json
```

### Custom Dockerfile

```dockerfile
FROM rust:latest as builder
RUN cargo install hotspots-cli

FROM debian:bookworm-slim
COPY --from=builder /usr/local/cargo/bin/hotspots /usr/local/bin/
WORKDIR /workspace
ENTRYPOINT ["hotspots"]
```

Build and use:
```bash
docker build -t hotspots:custom .
docker run --rm -v $(pwd):/workspace hotspots:custom analyze /workspace/src
```

---

## Environment Variables

Hotspots respects standard git and CI environment variables:

### Git Context

- `GIT_DIR` - Override `.git` directory
- `GIT_WORK_TREE` - Override working directory

### PR Detection (for delta mode)

**GitHub Actions:**
- `GITHUB_EVENT_NAME=pull_request`

**GitLab CI:**
- `CI_MERGE_REQUEST_IID`

**CircleCI:**
- `CIRCLE_PULL_REQUEST`

**Travis CI:**
- `TRAVIS_PULL_REQUEST`

When detected, delta mode compares vs. merge-base instead of direct parent.

---

## Exit Codes

| Code | Meaning | Triggered By |
|------|---------|--------------|
| `0` | Success | No violations, or only warnings |
| `1` | Failure | Blocking policy violations |

**Control exit behavior:**
```bash
# Fail on errors only (default)
hotspots analyze src/ --mode delta --policy

# Fail on warnings too
hotspots analyze src/ --mode delta --policy --fail-on warnings

# Never fail (for reporting only)
hotspots analyze src/ --mode delta --policy --fail-on never
```

**In CI scripts:**
```bash
# Ignore exit code (always pass)
hotspots analyze src/ --mode delta --policy || true

# Custom handling
if ! hotspots analyze src/ --mode delta --policy; then
  echo "Hotspots found violations, but continuing..."
fi
```

---

## Caching Strategies

### GitHub Actions

```yaml
- name: Cache Hotspots Binary
  uses: actions/cache@v4
  with:
    path: ~/.cargo/bin/hotspots
    key: hotspots-${{ runner.os }}-v1.0.0

- name: Install or Use Cached Hotspots
  run: |
    if ! command -v hotspots &> /dev/null; then
      cargo install hotspots-cli --version 1.0.0
    fi
```

### GitLab CI

```yaml
cache:
  key: hotspots-${CI_COMMIT_REF_SLUG}
  paths:
    - ~/.cargo/bin/hotspots
    - .hotspots/snapshots/
```

### CircleCI

```yaml
- restore_cache:
    keys:
      - hotspots-v1-{{ .Branch }}
      - hotspots-v1-

- save_cache:
    key: hotspots-v1-{{ .Branch }}
    paths:
      - ~/.cargo/bin/hotspots
```

---

## Multi-Language Projects

### Analyze Multiple Directories

```yaml
- name: Analyze TypeScript
  run: hotspots analyze src/ --mode delta --policy

- name: Analyze Go
  run: hotspots analyze backend/ --mode delta --policy

- name: Analyze Python
  run: hotspots analyze scripts/ --mode delta --policy
```

### Combined Report

```bash
# Analyze all at once (recommended)
hotspots analyze . --config .hotspotsrc.json --mode delta --policy
```

`.hotspotsrc.json`:
```json
{
  "include": [
    "src/**/*.ts",
    "backend/**/*.go",
    "scripts/**/*.py"
  ],
  "exclude": [
    "**/*.test.*",
    "**/node_modules/**"
  ]
}
```

---

## Troubleshooting

### "not in a git repository"

**Cause:** Missing `.git` directory or shallow clone.

**Fix:** Use `fetch-depth: 0` in checkout:
```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0
```

### "failed to extract git context"

**Cause:** Missing git metadata (commit SHA, parents).

**Fix:** Ensure full git history:
```bash
git fetch --unshallow  # If shallow
git fetch origin main:main  # Fetch base branch
```

### "merge-base not found"

**Cause:** PR base branch not available.

**Fix (GitHub Actions):**
```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0
    ref: ${{ github.event.pull_request.head.ref }}
```

**Fix (GitLab CI):**
```yaml
before_script:
  - git fetch origin $CI_MERGE_REQUEST_TARGET_BRANCH_NAME
```

### Exit Code Always 0

**Cause:** Using `|| true` or `fail-on: never`.

**Fix:** Remove exit code overrides:
```bash
# Bad
hotspots analyze src/ --mode delta --policy || true

# Good
hotspots analyze src/ --mode delta --policy
```

---

## Best Practices

### 1. Use Delta Mode in CI

```yaml
# ✅ Good - Delta mode for PRs
- run: hotspots analyze src/ --mode delta --policy

# ❌ Bad - Snapshot mode without comparison
- run: hotspots analyze src/ --format json
```

### 2. Separate Dev and CI Configs

**Development:** `.hotspotsrc.json`
```json
{
  "min_lrs": 0.0,
  "top": 20
}
```

**CI:** `.hotspots.ci.json`
```json
{
  "min_lrs": 5.0,
  "thresholds": {
    "moderate": 5.0,
    "high": 8.0,
    "critical": 10.0
  }
}
```

Use in CI:
```yaml
- run: hotspots analyze src/ --config .hotspots.ci.json --mode delta --policy
```

### 3. Archive Reports

```yaml
- uses: actions/upload-artifact@v4
  if: always()
  with:
    name: hotspots-report-${{ github.sha }}
    path: .hotspots/report.html
    retention-days: 30
```

### 4. Post PR Comments

```yaml
- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    post-comment: true  # Default for PRs
    github-token: ${{ secrets.GITHUB_TOKEN }}
```

### 5. Gradual Rollout

Start with warnings only:
```yaml
- run: hotspots analyze src/ --mode delta --policy --fail-on never
```

Then tighten:
```yaml
- run: hotspots analyze src/ --mode delta --policy --fail-on error
```

---

## Related Documentation

- [GitHub Action Guide](./github-action.md) - Complete GitHub Action documentation
- [Configuration](./configuration.md) - Config file format and options
- [CLI Reference](../reference/cli.md) - All CLI commands and flags
- [Output Formats](./output-formats.md) - JSON schema and HTML reports
- [Policy Engine](./usage.md#policy-engine) - Policy rules and enforcement

---

## Examples Repository

See [examples/ci-cd/](https://github.com/Stephen-Collins-tech/hotspots/tree/main/examples/ci-cd) for complete working examples:
- GitHub Actions
- GitLab CI
- CircleCI
- Jenkins
- Docker

---

**Need help?** Open an issue on [GitHub](https://github.com/Stephen-Collins-tech/hotspots/issues).
