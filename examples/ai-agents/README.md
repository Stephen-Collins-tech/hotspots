# Hotspots AI Agent Examples

Reference implementations demonstrating AI-assisted development workflows with Hotspots complexity analysis.

## Overview

These examples show how to integrate Hotspots into AI workflows for:

- **Automated refactoring** - Iteratively reduce complexity with AI assistance
- **Pre-commit review** - Catch complexity violations before committing
- **Constrained generation** - Generate code within complexity limits
- **PR automation** - Review pull requests for complexity regressions

Each example is a complete, runnable TypeScript program with clear documentation.

## Prerequisites

- **Node.js** 18+ and npm
- **Hotspots binary** installed and in PATH
- **API keys** for AI services (OpenAI, Anthropic)

Install Hotspots:

```bash
# From project root
cargo build --release
sudo cp target/release/hotspots /usr/local/bin/
```

Or use the development binary:

```bash
export HOTSPOTS_PATH="$(pwd)/target/release/hotspots"
```

## Installation

From the `examples/ai-agents` directory:

```bash
npm install
```

This installs:
- `@hotspots/types` - TypeScript type definitions
- `execa` - Process execution
- `openai` - OpenAI API client
- `@anthropic-ai/sdk` - Anthropic Claude API client
- `tsx` - TypeScript execution

## Directory Structure

```
examples/ai-agents/
├── shared/                    # Shared utilities
│   ├── hotspots-client.ts    # Hotspots CLI wrapper
│   ├── ai-prompts.ts         # Prompt templates
│   └── result-parser.ts      # Output parsing helpers
├── refactor-loop/            # Iterative refactoring example
│   └── refactor-loop.ts
├── pre-commit-review/        # Pre-commit hook example
│   └── pre-commit-review.ts
├── constrained-generation/   # Complexity-constrained code gen
│   └── constrained-generation.ts
├── pr-reviewer/              # PR complexity review bot
│   └── pr-reviewer.ts
├── package.json
├── tsconfig.json
└── README.md                 # This file
```

## Examples

### 1. Refactor Loop

Iteratively refactors a high-complexity function until it meets a target LRS.

**Usage:**

```bash
export OPENAI_API_KEY="sk-..."
npm run refactor-loop ../../hotspots-core/src/complex_module.rs handleComplexCase
```

**What it does:**

1. Analyzes the target function
2. If LRS > target, asks AI for refactoring suggestions
3. Applies suggestions and re-analyzes
4. Repeats until LRS < target or max iterations reached

**See:** `refactor-loop/refactor-loop.ts`

### 2. Pre-Commit Review

Checks staged changes for complexity violations before allowing commit.

**Usage:**

```bash
npm run pre-commit
```

**What it does:**

1. Runs Hotspots in delta mode on staged changes
2. Checks for policy violations
3. Reports critical complexity functions
4. Exits with code 1 to block commit if violations found

**Git hook setup:**

```bash
# Create .git/hooks/pre-commit
cat > .git/hooks/pre-commit << 'EOF'
#!/bin/bash
cd examples/ai-agents && npm run pre-commit
EOF

chmod +x .git/hooks/pre-commit
```

**See:** `pre-commit-review/pre-commit-review.ts`

### 3. Constrained Generation

Generates code that meets complexity constraints, regenerating if needed.

**Usage:**

```bash
export OPENAI_API_KEY="sk-..."
npm run constrained-gen "function to validate user registration"
```

**What it does:**

1. Asks AI to generate code with LRS constraint in prompt
2. Analyzes generated code
3. If LRS > target, regenerates with stricter guidance
4. Repeats until code meets constraint or max attempts reached

**See:** `constrained-generation/constrained-generation.ts`

### 4. PR Reviewer

Reviews pull requests for complexity regressions (stub for GitHub Action integration).

**Usage:**

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
export GITHUB_TOKEN="ghp_..."
npm run pr-reviewer <pr-number>
```

**What it does:**

1. Fetches PR diff from GitHub
2. Runs Hotspots delta analysis
3. Uses Claude to generate review comments
4. Posts comments to PR via GitHub API

**See:** `pr-reviewer/pr-reviewer.ts`

## Shared Utilities

### HotspotsClient

Wrapper around Hotspots CLI for easy programmatic access.

```typescript
import { HotspotsClient } from './shared/hotspots-client.js';

const client = new HotspotsClient();
const analysis = await client.analyze({
  path: 'src/',
  mode: 'snapshot',
  minLrs: 6.0,
});

console.log(`Analyzed ${analysis.functions.length} functions`);
```

### AI Prompts

Pre-built prompt templates for common AI tasks.

```typescript
import { createRefactorPrompt } from './shared/ai-prompts.js';

const prompt = createRefactorPrompt(functionReport, targetLrs);
// Send to AI API...
```

### Result Parser

Helpers for extracting insights from Hotspots output.

```typescript
import { getHighRiskFunctions, formatFunctionList } from './shared/result-parser.js';

const highRisk = getHighRiskFunctions(analysis, 9.0);
console.log(formatFunctionList(highRisk));
```

## Environment Variables

- `OPENAI_API_KEY` - OpenAI API key (for GPT-4 examples)
- `ANTHROPIC_API_KEY` - Anthropic API key (for Claude examples)
- `HOTSPOTS_PATH` - Path to hotspots binary (default: 'hotspots' from PATH)
- `GITHUB_TOKEN` - GitHub token (for PR reviewer example)

## Development

**Build TypeScript:**

```bash
npm run build
```

**Run examples directly:**

```bash
tsx refactor-loop/refactor-loop.ts <args>
```

**Clean build artifacts:**

```bash
npm run clean
```

## Best Practices

1. **Cache results** - Hotspots is deterministic, cache by file hash
2. **Use delta mode** - Faster and more focused for PRs
3. **Validate with tests** - Always run tests after AI changes
4. **Set reasonable limits** - Don't aim for LRS 0, moderate complexity is fine
5. **Review AI suggestions** - Don't blindly accept, verify correctness

## Customization

These examples are templates. Customize for your workflow:

- Adjust `targetLrs` thresholds
- Add custom prompts for your domain
- Integrate with your CI/CD system
- Add test validation steps
- Implement rollback on failure

## Troubleshooting

**"Hotspots binary not found"**

Set `HOTSPOTS_PATH` environment variable or add hotspots to PATH.

**"Cannot find module '@hotspots/types'"**

Run `npm install` from the `examples/ai-agents` directory.

**"API key not set"**

Export the required API key:

```bash
export OPENAI_API_KEY="sk-..."
export ANTHROPIC_API_KEY="sk-ant-..."
```

**TypeScript errors**

Rebuild:

```bash
npm run clean && npm run build
```

## See Also

- [AI Integration Guide](../../docs/AI_INTEGRATION.md) - Complete AI integration documentation
- [JSON Schema Reference](../../docs/json-schema.md) - Hotspots output format
- [TypeScript Types](../../packages/types/README.md) - @hotspots/types documentation
- [MCP Server](../../packages/mcp-server/README.md) - Claude Desktop integration

## License

MIT - Same as parent project
