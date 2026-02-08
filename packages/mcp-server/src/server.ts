import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from '@modelcontextprotocol/sdk/types.js';
import { analyze, AnalyzeInput } from './tools/analyze.js';

/**
 * Create and configure the MCP server
 */
export function createServer(): Server {
  const server = new Server(
    {
      name: 'hotspots-mcp-server',
      version: '1.0.0',
    },
    {
      capabilities: {
        tools: {},
      },
    }
  );

  // List available tools
  server.setRequestHandler(ListToolsRequestSchema, async () => {
    return {
      tools: [
        {
          name: 'hotspots_analyze',
          description:
            'Analyze JavaScript/TypeScript files for complexity metrics. ' +
            'Returns function-level complexity scores (LRS), risk bands, and policy violations. ' +
            'Use this to identify complex code that may need refactoring.',
          inputSchema: {
            type: 'object',
            properties: {
              path: {
                type: 'string',
                description: 'Path to file or directory to analyze (required)',
              },
              mode: {
                type: 'string',
                enum: ['snapshot', 'delta'],
                description:
                  'Analysis mode: "snapshot" for full analysis, "delta" for changed functions only (default: snapshot)',
              },
              minLrs: {
                type: 'number',
                description:
                  'Minimum LRS threshold - only return functions with LRS >= this value (optional)',
              },
              config: {
                type: 'string',
                description: 'Path to hotspots config file (optional)',
              },
            },
            required: ['path'],
          },
        },
      ],
    };
  });

  // Handle tool execution
  server.setRequestHandler(CallToolRequestSchema, async (request) => {
    const { name, arguments: args } = request.params;

    if (name === 'hotspots_analyze') {
      const input = (args || {}) as unknown as AnalyzeInput;
      const result = await analyze(input);

      if (!result.success) {
        return {
          content: [
            {
              type: 'text',
              text: `Error: ${result.error}`,
            },
          ],
          isError: true,
        };
      }

      return {
        content: [
          {
            type: 'text',
            text: result.summary || 'Analysis complete',
          },
          {
            type: 'text',
            text: `\n\n## Full JSON Output\n\n\`\`\`json\n${JSON.stringify(
              result.output,
              null,
              2
            )}\n\`\`\``,
          },
        ],
      };
    }

    throw new Error(`Unknown tool: ${name}`);
  });

  return server;
}

/**
 * Start the MCP server
 */
export async function startServer(): Promise<void> {
  const server = createServer();
  const transport = new StdioServerTransport();

  await server.connect(transport);

  // Log to stderr (stdout is used for MCP protocol)
  console.error('Hotspots MCP server started');
  console.error('Waiting for requests...');
}
