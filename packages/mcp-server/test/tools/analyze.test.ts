import { execFileSync } from 'node:child_process';
import Database from 'better-sqlite3';
import { describe, expect, it } from 'vitest';
import { handler, inputSchema, summarize } from '../../src/tools/analyze.js';
import { hasBinary, makeFixtureRepo } from '../helpers.js';

describe('analyze input validation', () => {
  it('requires path and validates touchMode', () => {
    expect(inputSchema.safeParse({}).success).toBe(false);
    expect(inputSchema.safeParse({ path: '.', touchMode: 'file', minLrs: 3 }).success).toBe(true);
    expect(inputSchema.safeParse({ path: '.', touchMode: 'hybrid:5' }).success).toBe(false);
  });
  it('summarizes triage buckets (fire/debt/watch/ok)', () => {
    const out = { triage: { fire: { count: 1 }, debt: { count: 2 }, watch: { count: 3, top: [{ lrs: 7.5 }, { lrs: 2 }] }, ok: { count: 4 } } };
    expect(summarize(out)).toEqual({ criticalCount: 1, highCount: 2, watchCount: 3, okCount: 4, maxLrs: 7.5 });
  });
});

describe.skipIf(!hasBinary)('analyze integration', () => {
  it('matches the CLI snapshot JSON, writes one row, and can re-sample the same commit', async () => {
    const repo = makeFixtureRepo();
    const args = { path: repo };
    const res = await handler(args);
    await handler(args); // snapshots are immutable per commit; --no-persist keeps re-sampling safe
    expect(res.isError).toBeUndefined();
    const cli = JSON.parse(execFileSync(process.env.HOTSPOTS_PATH || 'hotspots',
      ['analyze', repo, '--mode', 'snapshot', '--format', 'json', '--no-persist'],
      { cwd: repo, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }));
    expect(JSON.parse(res.content[0].text)).toEqual(cli);

    const db = new Database(`${repo}/.hotspots/mcp-samples.db`, { readonly: true });
    const rows = db.prepare("SELECT * FROM samples WHERE tool='analyze'").all() as any[];
    expect(rows).toHaveLength(2);
    const s = JSON.parse(rows[0].summary_json);
    expect(s.watchCount + s.okCount + s.criticalCount + s.highCount).toBe(2);
    expect(s.maxLrs).toBeGreaterThan(0);
  });

  it('returns a tool error for a bad path', async () => {
    const r = await handler({ path: '/nonexistent/repo/xyz' });
    expect(r.isError).toBe(true);
  });
});
