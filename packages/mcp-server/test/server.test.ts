import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { describe, expect, it } from 'vitest';
import { createServer } from '../src/server.js';
import { hasBinary, makeFixtureRepo } from './helpers.js';

async function connect() {
  const [a, b] = InMemoryTransport.createLinkedPair();
  await createServer().connect(a);
  const client = new Client({ name: 't', version: '0' });
  await client.connect(b);
  return client;
}

describe('MCP server', () => {
  it('registers the default tools (policy_check is opt-in) with usage-oriented descriptions', async () => {
    const { tools } = await (await connect()).listTools();
    expect(tools.map((t) => t.name).sort()).toEqual([
      'hotspots_analyze', 'hotspots_coordinate', 'hotspots_review_brief',
      'hotspots_sample_history', 'hotspots_sample_trends',
    ]);
    for (const t of tools) expect(t.description!.length).toBeGreaterThan(60);
  });

  it('registers hotspots_policy_check only when HOTSPOTS_MCP_POLICY_CHECK=1', async () => {
    process.env.HOTSPOTS_MCP_POLICY_CHECK = '1';
    try {
      const { tools } = await (await connect()).listTools();
      expect(tools.map((t) => t.name)).toContain('hotspots_policy_check');
    } finally {
      delete process.env.HOTSPOTS_MCP_POLICY_CHECK;
    }
  });

  it('tells the agent to present the brief, not act on it', async () => {
    const { tools } = await (await connect()).listTools();
    const t = tools.find((x) => x.name === 'hotspots_review_brief')!;
    expect(t.description).toMatch(/PRESENT THIS BRIEF TO THE USER/);
    expect(t.description).toMatch(/Do not act on it/);
  });

  it.skipIf(!hasBinary)('round-trips a coordinate call over MCP', async () => {
    const repo = makeFixtureRepo();
    const client = await connect();
    const r: any = await client.callTool({ name: 'hotspots_coordinate', arguments: { path: repo, files: ['src/a.ts'] } });
    expect(JSON.parse(r.content[0].text).coordinate.input_files).toEqual(['src/a.ts']);
    const h: any = await client.callTool({ name: 'hotspots_sample_history', arguments: { repoPath: repo } });
    expect(JSON.parse(h.content[0].text)).toHaveLength(1);
  });
});
