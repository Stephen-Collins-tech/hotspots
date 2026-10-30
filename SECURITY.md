# Security Policy

## Privacy & Data Policy

**Hotspots never transmits your source code, analysis results, file paths or metrics.**

When you run `hotspots`, it:
- Reads source files from your local filesystem
- Invokes `git log` and related git subprocesses locally
- Writes results to your local filesystem (`.hotspots/` directory)

The only network requests the CLI can make are:
- **Update check:** a request to the GitHub releases API for the latest version, at most once
  per 24 hours (cached in `~/.hotspots/update_check.json`). It sends nothing about you or
  your code.
- **Anonymous usage ping (opt-in, off by default):** only if you enable it. At most one small
  request per day to `hotspots.dev` containing the CLI version, OS, architecture, install
  channel, command name and a random install id. It is never sent in CI, and `DO_NOT_TRACK=1`
  or `HOTSPOTS_TELEMETRY=0` disables it regardless of settings. Exact payload and controls:
  [docs/TELEMETRY.md](docs/TELEMETRY.md).

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
