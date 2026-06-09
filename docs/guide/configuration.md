# Configuration

Configure Hotspots behavior using JSON configuration files.

## Config File Locations

Hotspots searches for configuration in the following order:

1. **Explicit path** - Via `--config` CLI flag
2. **`.hotspotsrc.json`** - Recommended location (project root)
3. **`hotspots.config.json`** - Alternative location (project root)
4. **`package.json`** - Under `"hotspots"` key

The first config found wins. If no config is found, default values are used.

## Basic Example

Create `.hotspotsrc.json` in your project root:

```json
{
  "exclude": [
    "**/*.test.ts",
    "**/*.test.tsx",
    "**/__tests__/**",
    "**/node_modules/**"
  ],
  "min_lrs": 3.0,
  "top": 50
}
```

## Complete Configuration Reference

### File Filtering

#### `include`
Glob patterns for files to include (default: all supported extensions).

```json
{
  "include": [
    "src/**/*.ts",
    "lib/**/*.js"
  ]
}
```

**Type:** `string[]`
**Default:** `[]` (include all supported files)

#### `exclude`
Glob patterns for files to exclude.

```json
{
  "exclude": [
    "**/*.test.ts",
    "**/*.test.tsx",
    "**/*.test.js",
    "**/*.test.jsx",
    "**/*.spec.ts",
    "**/*.spec.tsx",
    "**/*.spec.js",
    "**/*.spec.jsx",
    "**/node_modules/**",
    "**/__tests__/**",
    "**/__mocks__/**",
    "**/dist/**",
    "**/build/**",
    "**/vendor/**",
    "**/*.pb.go",
    "**/zz_generated*.go"
  ]
}
```

**Type:** `string[]`
**Default:** Test files, node_modules, dist, build, Go vendor/generated

### Risk Band Thresholds

#### `thresholds`
Customize LRS (Leverage Risk Score) thresholds for risk bands.

```json
{
  "thresholds": {
    "moderate": 3.0,
    "high": 6.0,
    "critical": 9.0
  }
}
```

**Fields:**
- `moderate` - LRS threshold for moderate risk (default: `3.0`)
- `high` - LRS threshold for high risk (default: `6.0`)
- `critical` - LRS threshold for critical risk (default: `9.0`)

**Validation:**
- All thresholds must be positive
- Must be ordered: `moderate < high < critical`

**Risk Bands:**
- **Low:** LRS < `moderate`
- **Moderate:** `moderate` ≤ LRS < `high`
- **High:** `high` ≤ LRS < `critical`
- **Critical:** LRS ≥ `critical`

### Metric Weights

#### `weights`
Customize how metrics contribute to LRS calculation.

```json
{
  "weights": {
    "cc": 1.0,
    "nd": 0.8,
    "fo": 0.6,
    "ns": 0.7
  }
}
```

**Fields:**
- `cc` - Cyclomatic Complexity weight (default: `1.0`)
- `nd` - Nesting Depth weight (default: `0.8`)
- `fo` - Fan-Out weight (default: `0.6`)
- `ns` - Non-Structured exits weight (default: `0.7`)

**Validation:**
- All weights must be non-negative (≥ 0.0)
- At least one weight must be positive (> 0.0)
- Weights cannot exceed 10.0

Weights scale the log-transformed contribution of each metric to LRS. See [LRS Specification](/reference/lrs-spec) for the full formula.

### Warning Thresholds

#### `warning_thresholds`
Configure proactive warning thresholds for policy engine.

```json
{
  "warning_thresholds": {
    "watch_min": 2.5,
    "watch_max": 3.0,
    "attention_min": 5.5,
    "attention_max": 6.0,
    "rapid_growth_percent": 50.0
  }
}
```

**Fields:**
- `watch_min` - Lower bound for "watch" range (default: `2.5`)
- `watch_max` - Upper bound for "watch" range (default: `3.0`)
- `attention_min` - Lower bound for "attention" range (default: `5.5`)
- `attention_max` - Upper bound for "attention" range (default: `6.0`)
- `rapid_growth_percent` - Percent increase threshold (default: `50.0`)

**Validation:**
- All thresholds must be positive
- Must be ordered: `watch_min < watch_max ≤ moderate < attention_min < attention_max ≤ high`

### Output Filtering

#### `min_lrs`
Minimum LRS to report (filter out low-complexity functions).

```json
{
  "min_lrs": 3.0
}
```

**Type:** `number`
**Default:** `0.0` (report all functions)

#### `top`
Maximum number of functions to show.

```json
{
  "top": 50
}
```

**Type:** `number`
**Default:** No limit (show all)

### Co-Change Analysis

#### `co_change_window_days`
Number of days of git history to mine for co-change pairs.

```json
{
  "co_change_window_days": 180
}
```

**Type:** `number` (integer ≥ 1)
**Default:** `90`

Projects with a slow commit cadence (e.g. once a week) benefit from a larger window.

#### `co_change_min_count`
Minimum number of co-changes required to report a pair. Pairs that appear fewer times are
filtered out as noise.

```json
{
  "co_change_min_count": 5
}
```

**Type:** `number` (integer ≥ 1)
**Default:** `3`

High-traffic repositories (50+ commits/day) may want a higher threshold to reduce noise.

#### `driver_threshold_percentile`
Percentile of each metric that a function must exceed to receive a specific driver label.

```json
{
  "driver_threshold_percentile": 75
}
```

**Type:** integer 1–99
**Default:** `75`

At the default of 75, a function must have a cyclomatic complexity above the 75th percentile of
all functions in the snapshot to trigger the `high_complexity` label (i.e. top 25%). The same
percentile gate applies to `nd` (deep_nesting), `fan_out` (high_fanout_churning), `fan_in`
(high_fanin_complex), and `touch_count` (high_churn_low_cc, high_fanout_churning).

Compound checks:
- `high_churn_low_cc`: touch above Pth percentile **and** cc below the (100-P)th percentile
- `high_fanout_churning`: fan_out above Pth percentile **and** touch above the 50th percentile

`cyclic_dep` stays absolute — being in a cycle is binary, not distribution-relative.

**When to tune:**
- Small or uniform repos → lower to 50–60 so more functions get specific labels
- Large repos with high median complexity → raise to 85–90 to reduce noise

#### `per_function_touches`
Whether to use per-function `git log -L` for touch metrics instead of file-level batching.

```json
{
  "per_function_touches": false
}
```

**Type:** `boolean`
**Default:** `true`

Per-function touch metrics are more accurate (each function gets its own 30-day touch count
rather than sharing the file's count). Warm runs use the on-disk cache
(`.hotspots/touch-cache.json.zst`) and match file-level speed (~230 ms vs ~268 ms on this
repo). The first run on a new commit is slow (~6 s for ~200 functions); subsequent runs are
fast. Set to `false` to always use file-level batching (useful in CI without a persistent
cache layer).

For very large repositories (50k+ functions), consider skipping touch metrics entirely with
the `--skip-touch-metrics` CLI flag. This avoids all git log I/O and can reduce analysis time
significantly (e.g. ~66 s savings on expo/expo). Touch counts will be reported as `0`.

## Complete Example

```json
{
  "include": [
    "src/**/*.ts",
    "src/**/*.tsx"
  ],
  "exclude": [
    "**/*.test.ts",
    "**/*.test.tsx",
    "**/__tests__/**",
    "**/node_modules/**",
    "**/dist/**",
    "**/coverage/**"
  ],
  "thresholds": {
    "moderate": 4.0,
    "high": 8.0,
    "critical": 12.0
  },
  "weights": {
    "cc": 1.0,
    "nd": 0.9,
    "fo": 0.5,
    "ns": 0.8
  },
  "warning_thresholds": {
    "watch_min": 3.5,
    "watch_max": 4.0,
    "attention_min": 7.5,
    "attention_max": 8.0,
    "rapid_growth_percent": 40.0
  },
  "min_lrs": 3.0,
  "top": 100
}
```

## Using in package.json

Add configuration under `"hotspots"` key:

```json
{
  "name": "my-project",
  "version": "1.0.0",
  "hotspots": {
    "exclude": [
      "**/*.test.ts",
      "**/node_modules/**"
    ],
    "min_lrs": 3.0
  }
}
```

## CLI Override

Config file settings can be overridden by CLI flags:

```bash
# Config file says min_lrs: 3.0, but CLI overrides to 5.0
hotspots analyze src/ --min-lrs 5.0

# Use specific config file
hotspots analyze src/ --config custom-config.json
```

**CLI flags take precedence over config file values.**

## Environment-Specific Configs

### Development

`.hotspotsrc.json`:
```json
{
  "exclude": ["**/*.test.ts"],
  "min_lrs": 0.0,
  "top": 20
}
```

### CI/CD

`hotspots.ci.json`:
```json
{
  "exclude": ["**/*.test.ts"],
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
- run: hotspots analyze src/ --config hotspots.ci.json --policy --fail-on blocking
```

## Configuration Validation

Hotspots validates configuration on load:

**Valid:**
```json
{
  "thresholds": {
    "moderate": 3.0,
    "high": 6.0,
    "critical": 9.0
  }
}
```

**Invalid (not ordered):**
```json
{
  "thresholds": {
    "moderate": 6.0,
    "high": 3.0,  // ❌ Error: high must be > moderate
    "critical": 9.0
  }
}
```

**Invalid (negative weight):**
```json
{
  "weights": {
    "cc": -1.0  // ❌ Error: weights must be non-negative
  }
}
```

## Default Values

If no config file is found, these defaults are used:

```json
{
  "include": [],
  "exclude": [
    "**/*.test.ts",
    "**/*.test.tsx",
    "**/*.test.js",
    "**/*.test.jsx",
    "**/*.spec.ts",
    "**/*.spec.tsx",
    "**/*.spec.js",
    "**/*.spec.jsx",
    "**/node_modules/**",
    "**/__tests__/**",
    "**/__mocks__/**",
    "**/dist/**",
    "**/build/**",
    "**/vendor/**",
    "**/*.pb.go",
    "**/zz_generated*.go"
  ],
  "thresholds": {
    "moderate": 3.0,
    "high": 6.0,
    "critical": 9.0
  },
  "weights": {
    "cc": 1.0,
    "nd": 0.8,
    "fo": 0.6,
    "ns": 0.7
  },
  "warning_thresholds": {
    "watch_min": 2.5,
    "watch_max": 3.0,
    "attention_min": 5.5,
    "attention_max": 6.0,
    "rapid_growth_percent": 50.0
  },
  "min_lrs": 0.0,
  "top": null
}
```

## Troubleshooting

### Config not being loaded

Check the config file is in project root:
```bash
ls -la .hotspotsrc.json
```

Verify JSON syntax:
```bash
cat .hotspotsrc.json | jq .
```

### Unknown fields error

Hotspots rejects unknown fields to catch typos:

```json
{
  "min_lrs": 3.0,
  "minLRS": 5.0  // ❌ Error: unknown field
}
```

Use exact field names from this guide.

### Config validation fails

Read the error message carefully - it tells you exactly what's wrong:

```
Error: thresholds.moderate (6.0) must be less than thresholds.high (5.0)
```

Fix the ordering and try again.

## Related Documentation

- [CLI Reference](../reference/cli.md) - Command-line options
- [Metrics & LRS](../reference/metrics.md) - How LRS is calculated
- [Suppression Comments](../guide/suppression.md) - Excluding functions from policy
- [Policy Engine](../guide/usage.md#policy-engine) - Using policies
