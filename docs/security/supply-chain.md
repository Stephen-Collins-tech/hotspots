# Hotspots Supply Chain Security Requirements

**Status:** Proposed  
**Target Repo:** `hotspots` (CLI) + `hotspots-cloud` (pipeline)  
**Audience:** CTOs at financial services companies; enterprise buyers evaluating open-core tooling

---

## Motivation

Developers evaluating a CLI tool that reads their codebase have two primary concerns: "does this tool send my code anywhere?" and "can I verify this binary is legitimate?" Enterprise buyers in financial services add a third: "is there a verifiable, auditable supply chain posture?" This document specifies the scanning, artifact, and CI/CD requirements that allow Hotspots to answer all three with evidence rather than assertion.

---

## Scope

| Component | In Scope |
|-----------|----------|
| `hotspots` CLI (Rust) | Yes — primary artifact |
| `hotspots-cloud` pipeline | Yes — CI/CD and release automation |
| `hotspots-content` public repo | No — static content only |
| Cloudflare R2 artifacts | Out of scope for this document |

---

## REQ-SEC-01 · Dependency Auditing via `cargo-audit`

**Tool:** [`cargo-audit`](https://github.com/rustsec/rustsec/tree/main/cargo-audit)  
**Database:** [RustSec Advisory Database](https://rustsec.org/)

### Requirements

- `cargo-audit` MUST run on every push to `main` and every pull request.
- The workflow MUST fail (non-zero exit) if any advisory is rated **critical** or **high**.
- Medium and low advisories MUST produce a warning annotation but MUST NOT block merge.
- The audit report MUST be uploaded as a GitHub Actions artifact retained for 30 days.
- A `cargo-audit` config file (`audit.toml`) MUST be committed to the repo root to track any intentional ignores with justification comments.

### Rationale

`cargo-audit` is the Rust-native standard, querying the RustSec Advisory Database which is maintained by the Rust Security Response WG. It provides deterministic Cargo.lock-based scanning with no external service dependency.

---

## REQ-SEC-02 · Vulnerability and Secrets Scanning via Trivy

**Tool:** [`trivy`](https://github.com/aquasecurity/trivy) (Aqua Security, OSS)

### Requirements

- Trivy MUST run in `fs` mode scanning the full repository on every push to `main` and every pull request.
- Scanners enabled: `vuln` (Cargo.lock dependencies), `secret` (hardcoded credentials/tokens), `misconfig`.
- The workflow MUST fail on any `CRITICAL` severity finding in either vuln or secret categories.
- `HIGH` severity findings MUST produce a blocking annotation with a 7-day remediation SLA before they become release-blocking.
- Trivy results MUST be output in SARIF format and uploaded to the GitHub Security tab via `github/codeql-action/upload-sarif`.
- A `.trivyignore` file MAY be committed to suppress known false positives; each entry MUST include an expiry date and justification comment.

### Rationale

Trivy has first-class Rust/Cargo support, handles secrets scanning in the same pass, and integrates natively with GitHub Advanced Security's SARIF upload pathway. This eliminates the need for a separate secrets scanner while producing results visible in the GitHub Security tab for external auditors.

---

## REQ-SEC-03 · Git History Secrets Scanning via Gitleaks

**Tool:** [`gitleaks`](https://github.com/gitleaks/gitleaks)

### Requirements

- Gitleaks MUST run a full history scan (`--log-opts="--all"`) on a weekly scheduled workflow.
- The weekly full scan MUST notify via a GitHub issue (auto-created) if findings are detected.
- A `.gitleaks.toml` config file MUST be committed to the repo root. Any allowlisted patterns MUST include a justification comment.
- Gitleaks results from the weekly scan MUST be retained as a GitHub Actions artifact for 90 days.
- Gitleaks SHOULD NOT be run on every PR — Trivy's `secret` scanner (REQ-SEC-02) already covers the working tree delta; duplicating it per-PR adds latency with no additional coverage.

### Rationale

Trivy's secret scanning covers the working tree; Gitleaks covers git history, which is the more common vector for accidentally committed secrets (API keys, tokens) that persist in commit history even after deletion from HEAD. Limiting Gitleaks to a weekly scheduled run avoids redundancy with Trivy while still providing periodic full-history coverage.

---

## REQ-SEC-04 · SBOM Generation

**Format:** CycloneDX (primary), SPDX (secondary)  
**Tool:** [`cargo-cyclonedx`](https://github.com/CycloneDX/cyclonedx-rust-cargo)

### Requirements

- An SBOM in CycloneDX JSON format MUST be generated as part of every release workflow.
- An SBOM in SPDX JSON format SHOULD also be generated for compatibility with federal/enterprise tooling.
- SBOMs MUST be attached as assets to every GitHub Release.
- SBOMs MUST include all direct and transitive dependencies with their versions and crates.io source URLs.
- SBOMs MUST NOT be generated on PR workflows — only on releases and `main` pushes (to avoid noise).
- The SBOM filename convention MUST follow: `hotspots-<version>-sbom.cdx.json` / `hotspots-<version>-sbom.spdx.json`.

### Rationale

SBOMs are now mandated by US Executive Order 14028 for software sold to federal agencies, and are increasingly required by enterprise procurement in financial services. Attaching them to GitHub Releases makes them discoverable without additional documentation burden.

---

## REQ-SEC-05 · Release Signing via Sigstore/cosign

**Tool:** [`cosign`](https://github.com/sigstore/cosign) (Sigstore)

### Requirements

- All release binaries (compiled artifacts for each target platform) MUST be signed using `cosign` keyless signing via OIDC (GitHub Actions identity — no long-lived private key required).
- Signatures MUST be recorded in the Sigstore transparency log (Rekor).
- A `cosign.pub` verification file OR a reference to the Rekor log entry MUST be published in the repo's `SECURITY.md`.
- The release workflow README MUST include a `cosign verify` invocation users can run to validate any downloaded binary.
- SBOMs (REQ-SEC-04) MUST also be signed with `cosign` and their signatures attached to the GitHub Release.

### Example verification command (to be included in SECURITY.md):
```bash
cosign verify-blob \
  --certificate-identity "https://github.com/Stephen-Collins-tech/hotspots/.github/workflows/release.yml@refs/tags/v*" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  --bundle hotspots-<version>-<platform>.bundle \
  hotspots-<version>-<platform>
```

### Rationale

Keyless signing via GitHub OIDC eliminates the operational burden of managing a signing key while providing cryptographic proof that a binary was produced by a specific, auditable CI workflow — not a compromised developer machine.

---

## REQ-SEC-06 · GitHub Security Tab Integration

### Requirements

- The GitHub repository MUST have GitHub Advanced Security enabled (available for public repos at no cost).
- All SARIF results from Trivy (REQ-SEC-02) MUST be uploaded to the Security tab on every push to `main`.
- Code scanning alerts MUST be triaged within 14 days of surfacing.
- Dependabot alerts MUST be enabled as a secondary signal (supplementary to `cargo-audit`).
- The Security tab MUST remain publicly visible to allow enterprise evaluators to self-serve audit results without contacting the maintainer.

---

## REQ-SEC-07 · README Security Badge

### Requirements

- The `hotspots` repo README MUST include a security scan status badge linking to the CI workflow.
- The badge MUST reflect the status of the combined security workflow (audit + Trivy + Gitleaks).
- The badge MUST link directly to the most recent workflow run for transparency.

### Example badge markup:
```markdown
[![Security Scan](https://github.com/Stephen-Collins-tech/hotspots/actions/workflows/security.yml/badge.svg)](https://github.com/Stephen-Collins-tech/hotspots/actions/workflows/security.yml)
```

---

## REQ-SEC-08 · SECURITY.md

A `SECURITY.md` file MUST be present in the repo root and MUST include:

- **Privacy and data policy** — an explicit statement that Hotspots runs entirely locally, makes no network requests, and does not transmit source code, analysis results, or telemetry to any external service. This is the first question a security-conscious developer asks about a CLI tool that reads their codebase.
- Supported versions policy (which versions receive security fixes)
- Vulnerability disclosure / responsible disclosure process and contact (e.g., GitHub private vulnerability reporting)
- Link to the most recent SBOM release asset
- `cosign verify` instructions (from REQ-SEC-05)
- Link to the GitHub Security tab for public audit trail
- Description of the scanning stack (cargo-audit, Trivy, Gitleaks) with links

---

## Implementation Order (Suggested)

| Phase | Requirements | Effort | Signal Value |
|-------|-------------|--------|--------------|
| 1 | REQ-SEC-01, REQ-SEC-02 | Low — GitHub Actions workflows | Foundational scanning in CI; Trivy covers both vuln and secrets |
| 2 | REQ-SEC-06, REQ-SEC-07, REQ-SEC-08 | Low — config + badge + docs | Immediately visible to any developer evaluating the tool; privacy policy answers the #1 developer concern |
| 3 | REQ-SEC-03 | Low — one scheduled workflow | Weekly full-history secrets scan; no per-PR overhead |
| 4 | REQ-SEC-05 | Medium — cosign integration | Signed binaries; strongest signal for security-aware developers and enterprise buyers |
| 5 | REQ-SEC-04 | Medium — release workflow changes | SBOM attached to releases; primarily valuable for enterprise/federal procurement |

---

## Non-Requirements (Explicitly Out of Scope)

- **Dynamic analysis / sandboxed execution** — not practical for a CLI tool; static + history scanning is sufficient signal
- **Socket.dev / GuardDog** — both are primarily npm/PyPI ecosystem tools with limited Rust support; the cargo-audit + Trivy stack provides equivalent coverage natively
- **Container scanning** — Hotspots CLI is not distributed as a container image; revisit if that changes
- **DAST** — not applicable to a CLI tool

---

*Last updated: 2026-03-07*
