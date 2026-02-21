# Installation

Install Hotspots on your system.

## Quick Install

### macOS / Linux

```bash
curl -fsSL https://hotspots.dev/install.sh | sh
```

This installs the binary to `~/.local/bin/hotspots` and prints a PATH reminder if needed.

**Install a specific version:**
```bash
HOTSPOTS_VERSION=v1.0.0 curl -fsSL https://hotspots.dev/install.sh | sh
```

Verify installation:
```bash
hotspots --version
```

### From Source (Rust Required)

```bash
git clone https://github.com/Stephen-Collins-tech/hotspots.git
cd hotspots
cargo build --release
mkdir -p ~/.local/bin
cp target/release/hotspots ~/.local/bin/
```

## Platform-Specific Instructions

### macOS (Homebrew)

Coming soon:
```bash
brew install hotspots
```

### Linux (Debian/Ubuntu)

Coming soon — `.deb` package

### Windows

Coming soon — Windows support

## GitHub Action

Use Hotspots in GitHub Actions without installing anything:

```yaml
- uses: Stephen-Collins-tech/hotspots@v1
  with:
    github-token: ${{ secrets.GITHUB_TOKEN }}
```

See [CI/CD & GitHub Action Guide](../guide/ci-cd.md) for complete usage.

## MCP Server (AI Integration)

> **Coming Soon** — Native MCP server integration is planned for a future release.

Until then, use Hotspots with Claude Code directly via CLI commands:

```bash
# Analyze changes in your project
hotspots analyze . --mode delta --format json

# Get agent-optimized output (quadrant buckets + action text)
hotspots analyze . --mode delta --all-functions --format json
```

See [AI Integration Guide](../integrations/ai-integration.md) for complete AI workflow documentation.

## Verify Installation

```bash
hotspots --version
hotspots analyze --help
```

## Next Steps

- [Quick Start Guide](./quick-start.md) — 5-minute tutorial
- [Usage Guide](../guide/usage.md) — Complete CLI reference
- [Configuration](../guide/configuration.md) — Config file setup

## Troubleshooting

### Command not found

Ensure `~/.local/bin` is in your PATH:

```bash
echo $PATH
```

If it's missing, add to `~/.zshrc` or `~/.bashrc`:

```bash
export PATH="$HOME/.local/bin:$PATH"
source ~/.zshrc  # or source ~/.bashrc
```

### Build from source fails

Ensure you have Rust 1.75 or later:

```bash
rustc --version
rustup update stable
```

## Upgrading

```bash
curl -L https://github.com/Stephen-Collins-tech/hotspots/releases/latest/download/hotspots-$(uname -s)-$(uname -m) -o hotspots
chmod +x hotspots
mv hotspots ~/.local/bin/
hotspots --version
```

## Uninstall

```bash
rm ~/.local/bin/hotspots
```
