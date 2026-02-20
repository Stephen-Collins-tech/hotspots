# Installation

Install Hotspots on your system.

## Quick Install

### macOS / Linux

```bash
curl -L https://github.com/Stephen-Collins-tech/hotspots/releases/latest/download/hotspots-$(uname -s)-$(uname -m) -o hotspots
chmod +x hotspots
sudo mv hotspots /usr/local/bin/
```

Verify installation:
```bash
hotspots --version
```

### From Source (Rust Required)

```bash
# Clone the repository
git clone https://github.com/Stephen-Collins-tech/hotspots.git
cd hotspots

# Build with cargo
cargo build --release

# Install to system
sudo cp target/release/hotspots /usr/local/bin/

# Or add to PATH
export PATH="$PATH:$(pwd)/target/release"
```

## Platform-Specific Instructions

### macOS (Homebrew)

Coming soon:
```bash
brew install hotspots
```

### Linux (Debian/Ubuntu)

Coming soon - `.deb` package

### Windows

Coming soon - Windows support

## GitHub Action

Use Hotspots in GitHub Actions:

```yaml
- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    github-token: ${{ secrets.GITHUB_TOKEN }}
```

See [GitHub Action Guide](../guide/github-action.md) for more.

## MCP Server (AI Integration)

> **Coming Soon** — Native MCP server integration is planned for a future release.

Until then, use Hotspots with Claude Code directly via CLI commands:

```bash
# Analyze changes in your project
hotspots analyze . --mode delta --format json

# Get agent-optimized output (quadrant buckets + action text)
hotspots analyze . --mode delta --all-functions --format json
```

See [AI Integration Guide](../integrations/mcp-server.md) for complete AI workflow documentation.

## Verify Installation

Run a quick test:

```bash
hotspots analyze tests/fixtures/simple.ts
```

You should see complexity analysis output.

## Next Steps

- [Quick Start Guide](./quick-start.md) - 5-minute tutorial
- [Usage Guide](../guide/usage.md) - Complete CLI reference
- [Configuration](../guide/configuration.md) - Config file setup

## Troubleshooting

### Command not found

Ensure `/usr/local/bin` is in your PATH:

```bash
echo $PATH
```

Add to `~/.bashrc` or `~/.zshrc`:

```bash
export PATH="/usr/local/bin:$PATH"
```

### Permission denied

Use `sudo` for system-wide installation or install to user directory:

```bash
mkdir -p ~/.local/bin
mv hotspots ~/.local/bin/
export PATH="$HOME/.local/bin:$PATH"
```

### Build from source fails

Ensure you have Rust 1.75 or later:

```bash
rustc --version
rustup update stable
```

## Upgrading

```bash
# Download latest release
curl -L https://github.com/Stephen-Collins-tech/hotspots/releases/latest/download/hotspots-$(uname -s)-$(uname -m) -o hotspots
chmod +x hotspots
sudo mv hotspots /usr/local/bin/

# Verify new version
hotspots --version
```

## Uninstall

```bash
sudo rm /usr/local/bin/hotspots
```
