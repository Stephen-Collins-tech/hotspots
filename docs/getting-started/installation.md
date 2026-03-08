# Installation

Install Hotspots on your system.

## Quick Install

### macOS (Homebrew)

```bash
brew install Stephen-Collins-tech/tap/hotspots
```

### Linux

```bash
curl -fsSL https://raw.githubusercontent.com/Stephen-Collins-tech/hotspots/main/install.sh | sh
```

This installs the binary to `~/.local/bin/hotspots` and prints a PATH reminder if needed.

**Install a specific version:**
```bash
HOTSPOTS_VERSION=v1.0.0 curl -fsSL https://raw.githubusercontent.com/Stephen-Collins-tech/hotspots/main/install.sh | sh
```

### Windows

Download the latest binary from the [GitHub releases page](https://github.com/Stephen-Collins-tech/hotspots/releases/latest) and add it to your PATH.

### From Source (Rust Required)

```bash
git clone https://github.com/Stephen-Collins-tech/hotspots.git
cd hotspots
cargo build --release
mkdir -p ~/.local/bin
cp target/release/hotspots ~/.local/bin/
```

## GitHub Action

Use Hotspots in GitHub Actions without installing anything:

```yaml
- uses: Stephen-Collins-tech/hotspots@v1
  with:
    github-token: ${{ secrets.GITHUB_TOKEN }}
```

See [CI/CD & GitHub Action Guide](../guide/ci-cd.md) for complete usage.

## AI Integration

Use Hotspots with Claude Code directly via CLI commands:

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

**macOS (Homebrew):**
```bash
brew upgrade Stephen-Collins-tech/tap/hotspots
```

**Linux (curl install):**
```bash
curl -fsSL https://raw.githubusercontent.com/Stephen-Collins-tech/hotspots/main/install.sh | sh
```

## Uninstall

**macOS (Homebrew):**
```bash
brew uninstall hotspots
```

**Linux (curl install):**
```bash
rm ~/.local/bin/hotspots
```
