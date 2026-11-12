import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import * as coordinate from './tools/coordinate.js';
import * as analyze from './tools/analyze.js';
import * as reviewBrief from './tools/review-brief.js';
import * as policyCheck from './tools/policy-check.js';
import * as samples from './tools/samples.js';
import { fail, type ToolResult } from './types.js';

/** Turn thrown errors (missing binary, bad repo) into MCP tool errors instead of protocol errors. */
const safe = (fn: (a: unknown) => Promise<ToolResult>) => async (args: unknown): Promise<ToolResult> => {
  try {
    return await fn(args);
  } catch (e: any) {
    return fail(`${e?.name ?? 'Error'}: ${e?.message ?? e}`);
  }
};

export function createServer(): McpServer {
  const server = new McpServer({ name: '@hotspots/mcp-server', version: '0.1.0' });
  server.tool(reviewBrief.name, reviewBrief.description, reviewBrief.shape, safe(reviewBrief.handler));
  // Coordinate's shape is an object schema without the refine; the handler enforces exactly-one.
  server.tool(coordinate.name, coordinate.description, coordinate.shape, safe(coordinate.handler));
  server.tool(analyze.name, analyze.description, analyze.shape, safe(analyze.handler));
  // Not in the default set: the server leads with the review brief. Opt in to the policy gate.
  if (process.env.HOTSPOTS_MCP_POLICY_CHECK === '1') {
    server.tool(policyCheck.name, policyCheck.description, policyCheck.shape, safe(policyCheck.handler));
  }
  server.tool(samples.historyName, samples.historyDescription, samples.historyShape, safe(samples.historyHandler));
  server.tool(samples.trendsName, samples.trendsDescription, samples.trendsShape, safe(samples.trendsHandler));
  return server;
}

