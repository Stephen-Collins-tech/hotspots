# Installation

## Install options

### macOS (Homebrew)

```bash
brew install Stephen-Collins-tech/tap/hotspots
```

### npm

```bash
npm install -g @stephencollinstech/hotspots
```

Works on macOS, Linux, and Windows. No Rust toolchain required.

### pip

```bash
pip install hotspots-cli
```

Available on [PyPI](https://pypi.org/project/hotspots-cli/). Works on macOS, Linux, and Windows.

### Linux

```bash
curl -fsSL https://raw.githubusercontent.com/Stephen-Collins-tech/hotspots/main/install.sh | sh
```

Installs to `~/.local/bin/hotspots`. The script checks your current version first — if you're already up to date, it exits immediately. If an update is available, it shows the versions and asks before installing.

Install a specific version:

```bash
HOTSPOTS_VERSION=v1.23.0 curl -fsSL https://raw.githubusercontent.com/Stephen-Collins-tech/hotspots/main/install.sh | sh
```

### Any platform (Rust required)

```bash
cargo install hotspots-cli
```

Available on [crates.io](https://crates.io/crates/hotspots-cli). Works on macOS, Linux, and Windows.

### Windows

Download the latest binary from [GitHub Releases](https://github.com/Stephen-Collins-tech/hotspots/releases/latest) and add it to your PATH.

### GitHub Action

Use Hotspots in CI without installing anything locally:

```yaml
- uses: Stephen-Collins-tech/hotspots@v1
  with:
    github-token: ${{ secrets.GITHUB_TOKEN }}
```

See [CI/CD Guide](/guide/ci-cd) for full usage.

### Build from source

```bash
git clone https://github.com/Stephen-Collins-tech/hotspots.git
cd hotspots
cargo build --release
cp target/release/hotspots ~/.local/bin/
```

Requires Rust 1.75+.

---

## Verify

```bash
hotspots --version
hotspots analyze --help
```

---

## Upgrade

**macOS:**
```bash
brew upgrade Stephen-Collins-tech/tap/hotspots
```

**npm:**
```bash
npm update -g @stephencollinstech/hotspots
```

**pip:**
```bash
pip install --upgrade hotspots-cli
```

**Linux:**
```bash
curl -fsSL https://raw.githubusercontent.com/Stephen-Collins-tech/hotspots/main/install.sh | sh
```

---

## Troubleshoot

**`command not found` after Linux install** — `~/.local/bin` may not be in your PATH:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Add that line to your `~/.zshrc` or `~/.bashrc` to make it permanent.

**Build from source fails** — check your Rust version:

```bash
rustc --version   # need 1.75+
rustup update stable
```

---

## Next

[Quick Start →](/getting-started/quick-start) — analyze your first codebase in 5 minutes.
