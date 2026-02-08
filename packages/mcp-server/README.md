# @hotspots/mcp-server

Model Context Protocol (MCP) server for [Hotspots](https://hotspots.dev) complexity analysis.

This server allows AI assistants like Claude to run Hotspots complexity analysis directly during conversations, enabling real-time code quality insights and refactoring suggestions.

## Installation

```bash
npm install -g @hotspots/mcp-server
```

Or use directly with `npx`:

```bash
npx @hotspots/mcp-server
```

## Prerequisites

- Node.js 18+
- Hotspots CLI installed and available in PATH (`npm install -g hotspots` or build from source)

## Usage with Claude Desktop

Add to your Claude Desktop configuration file:

**macOS:** `~/Library/Application Support/Claude/claude_desktop_config.json`

**Windows:** `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "hotspots": {
      "command": "npx",
      "args": ["@hotspots/mcp-server"]
    }
  }
}
```

Or if installed globally:

```json
{
  "mcpServers": {
    "hotspots": {
      "command": "hotspots-mcp"
    }
  }
}
```

Restart Claude Desktop after updating the configuration.

## Usage with Claude Code

Claude Code automatically discovers MCP servers. No configuration needed if installed globally.

## Available Tools

### `hotspots_analyze`

Analyze JavaScript/TypeScript files for complexity metrics.

**Parameters:**
- `path` (required): Path to file or directory to analyze
- `mode` (optional): Analysis mode - "snapshot" (full) or "delta" (changes only)
- `minLrs` (optional): Minimum LRS threshold - only return functions with LRS >= this value
- `config` (optional): Path to hotspots config file

**Example:**

```
Analyze the src/ directory for complexity hotspots
```

Claude will call:
```json
{
  "path": "src/",
  "mode": "snapshot"
}
```

**Returns:**
- Human-readable summary with risk breakdown
- High-risk functions with LRS scores
- Policy violations (if any)
- Full JSON output with all metrics

## Example Conversation

```
You: "Analyze my TypeScript project for complexity issues"

Claude: *calls hotspots_analyze with path: "."*

Claude: "I analyzed your project and found 3 critical complexity hotspots:

1. handleUserAuthentication in src/auth.ts:45 - LRS: 9.8 (critical)
   - High cyclomatic complexity (CC: 15)
   - Deep nesting (ND: 4)
   - Consider breaking this into smaller functions

2. processPaymentTransaction in src/payment.ts:120 - LRS: 8.9 (critical)
   - Many function calls (FO: 12)
   - Multiple exit points (NS: 8)
   - Refactor to reduce dependencies

3. validateForm in src/forms.ts:67 - LRS: 7.5 (high)
   - Long chain of validations
   - Consider using a validation library

Would you like me to suggest specific refactorings for any of these?"
```

## Metrics Explained

- **LRS (Logarithmic Risk Score)**: Composite complexity metric (0-15+)
  - < 3.0: Low complexity
  - 3.0-6.0: Moderate complexity
  - 6.0-9.0: High complexity
  - ≥ 9.0: Critical complexity

- **CC (Cyclomatic Complexity)**: Number of decision points
- **ND (Nesting Depth)**: Maximum nesting level
- **FO (Fan-Out)**: Number of called functions
- **NS (Non-Structured exits)**: Early returns, throws, etc.

## Development

```bash
# Install dependencies
npm install

# Build
npm run build

# Watch mode
npm run dev

# Test locally
node dist/index.js
```

## Configuration

The MCP server will automatically find the `hotspots` binary in your PATH. To use a custom binary location, set the `HOTSPOTS_PATH` environment variable:

```json
{
  "mcpServers": {
    "hotspots": {
      "command": "npx",
      "args": ["@hotspots/mcp-server"],
      "env": {
        "HOTSPOTS_PATH": "/usr/local/bin/hotspots"
      }
    }
  }
}
```

## Troubleshooting

### "hotspots binary not found in PATH"

Make sure Hotspots is installed and available in your PATH:

```bash
which hotspots
# Should output: /usr/local/bin/hotspots (or similar)
```

If not installed:

```bash
npm install -g hotspots
# or build from source
```

### "Path does not exist"

Ensure you're providing a valid relative or absolute path. The path is resolved relative to the current working directory where Claude is running.

### MCP server not appearing in Claude

1. Check that Claude Desktop config file exists and is valid JSON
2. Restart Claude Desktop completely (quit and relaunch)
3. Check Claude Desktop logs for errors

## Related

- [Hotspots CLI](https://github.com/Stephen-Collins-tech/hotspots)
- [Hotspots GitHub Action](https://github.com/Stephen-Collins-tech/hotspots/tree/main/action)
- [@hotspots/types](https://www.npmjs.com/package/@hotspots/types) - TypeScript types
- [Model Context Protocol](https://modelcontextprotocol.io)

## License

MIT
