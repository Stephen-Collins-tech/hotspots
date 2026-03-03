Multi-language complexity analysis tool for CI/CD pipelines.

## Installation

### One-line install (macOS / Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/Stephen-Collins-tech/hotspots/main/install.sh | sh
```

Or install a specific version:

```bash
HOTSPOTS_VERSION=__VERSION__ curl -fsSL https://raw.githubusercontent.com/Stephen-Collins-tech/hotspots/main/install.sh | sh
```

### GitHub Action

```yaml
- uses: Stephen-Collins-tech/hotspots@__VERSION__
```

### Manual download

| Platform | Asset |
|----------|-------|
| macOS (Apple Silicon) | `hotspots-darwin-aarch64.tar.gz` |
| Linux x86_64 | `hotspots-linux-x86_64.tar.gz` |
| Windows x86_64 | `hotspots-windows-x86_64.zip` |

### Cargo

```bash
cargo install --git https://github.com/Stephen-Collins-tech/hotspots --tag __VERSION__
```

## Documentation

- [Getting Started](https://github.com/Stephen-Collins-tech/hotspots#readme)
- [GitHub Action Usage](https://github.com/Stephen-Collins-tech/hotspots/tree/main/action)
- [Configuration Guide](https://hotspots.dev)

---

