# Security Policy

## Privacy & Data Policy

**Hotspots makes no network requests.**

When you run `hotspots`, it:
- Reads source files from your local filesystem
- Invokes `git log` and related git subprocesses locally
- Writes results to your local filesystem (`.hotspots/` directory)

It does not transmit your source code, analysis results, file paths, metrics,
or any telemetry to any external server. There is no analytics, no update check,
and no remote endpoint of any kind. You can verify this by inspecting the
[dependency tree](https://github.com/Stephen-Collins-tech/hotspots/blob/main/Cargo.lock)
— there are no HTTP client dependencies.

---

## Supported Versions

| Version | Supported |
|---------|-----------|
| Latest (`main`) | Yes |
| Previous minor | Security fixes only |
| Older | No |

We follow [Semantic Versioning](https://semver.org/). Security fixes are released
as patch versions and tagged immediately.

---

## Reporting a Vulnerability

Please **do not** open a public GitHub issue for security vulnerabilities.

Use [GitHub's private vulnerability reporting](https://github.com/Stephen-Collins-tech/hotspots/security/advisories/new)
to report a vulnerability confidentially. We aim to acknowledge reports within
48 hours and provide a fix or mitigation within 14 days for critical issues.

---

## Security Scanning Stack

Every push to `main` and every pull request runs:

| Tool | What it checks |
|------|---------------|
| [`cargo-audit`](https://github.com/rustsec/rustsec/tree/main/cargo-audit) | Rust dependencies against the [RustSec Advisory Database](https://rustsec.org/) |
| [`trivy`](https://github.com/aquasecurity/trivy) | Dependency vulnerabilities, hardcoded secrets, and misconfigurations (SARIF uploaded to Security tab) |

A weekly scheduled scan runs:

| Tool | What it checks |
|------|---------------|
| [`gitleaks`](https://github.com/gitleaks/gitleaks) | Full git history for accidentally committed secrets |

Results are visible in the [GitHub Security tab](https://github.com/Stephen-Collins-tech/hotspots/security).

---

## Software Bill of Materials (SBOM)

SBOMs in CycloneDX and SPDX format are attached to every
[GitHub Release](https://github.com/Stephen-Collins-tech/hotspots/releases)
as `hotspots-<version>-sbom.cdx.json` and `hotspots-<version>-sbom.spdx.json`.

> **Note:** SBOM generation is planned for a future release. This section will be
> updated when SBOMs are available.

---

## Binary Verification (cosign)

Release binaries are signed using [Sigstore cosign](https://github.com/sigstore/cosign)
keyless signing via GitHub Actions OIDC. Signatures are recorded in the
[Rekor transparency log](https://rekor.sigstore.dev/).

To verify a downloaded binary:

```bash
cosign verify-blob \
  --certificate-identity "https://github.com/Stephen-Collins-tech/hotspots/.github/workflows/release.yml@refs/tags/v*" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  --bundle hotspots-<version>-<platform>.bundle \
  hotspots-<version>-<platform>
```

> **Note:** cosign signing is planned for a future release. Bundles will be
> attached to each GitHub Release once implemented.
