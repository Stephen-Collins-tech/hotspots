import { spawnSync } from 'node:child_process';
import Database from 'better-sqlite3';
import { describe, expect, it } from 'vitest';
import { EXIT_MEANINGS, handler, inputSchema, summarize } from '../../src/tools/policy-check.js';
import { hasBinary, makeFixtureRepo } from '../helpers.js';

describe('policy_check input validation', () => {
  it('requires path/base/head', () => {
    expect(inputSchema.safeParse({ path: '.', base: 'a' }).success).toBe(false);
    expect(inputSchema.safeParse({ path: '.', base: 'a', head: 'b', top: 5, autoAnalyze: true }).success).toBe(true);
    expect(inputSchema.safeParse({ path: '.', base: 'a', head: 'b', top: 0 }).success).toBe(false);
  });
  it('documents exit codes 0-3 and summarizes policy results', () => {
    expect(Object.keys(EXIT_MEANINGS)).toEqual(['0', '1', '2', '3']);
    expect(summarize({ delta: { policy: { failed: [{}], warnings: [{}, {}] } } })).toEqual({ blockingViolations: 1, warnings: 2 });
    expect(summarize({})).toEqual({ blockingViolations: 0, warnings: 0 });
  });
});

describe.skipIf(!hasBinary)('policy_check integration', () => {
  it('surfaces exit 1 with the CLI diff JSON and logs one row', async () => {
    const repo = makeFixtureRepo();
    const res = await handler({ path: repo, base: 'HEAD~1', head: 'HEAD', autoAnalyze: true });
    expect(res.isError).toBeUndefined();
    const body = JSON.parse(res.content[0].text);
    expect(body.exitCode).toBe(1);
    expect(body.exitMeaning).toMatch(/Blocking/);
    const cliRun = spawnSync(process.env.HOTSPOTS_PATH || 'hotspots',
      ['diff', 'HEAD~1', 'HEAD', '--policy', '--format', 'json', '--auto-analyze'], { cwd: repo, encoding: 'utf8' });
    expect(cliRun.status).toBe(1);
    const cli = JSON.parse(cliRun.stdout);
    expect(body.diff).toEqual(cli);

    const db = new Database(`${repo}/.hotspots/mcp-samples.db`, { readonly: true });
    const rows = db.prepare('SELECT * FROM samples').all() as any[];
    expect(rows).toHaveLength(1);
    expect(rows[0].tool).toBe('policy_check');
    expect(rows[0].exit_code).toBe(1);
    expect(JSON.parse(rows[0].summary_json).blockingViolations).toBeGreaterThan(0);
  });

  it('reports a bad ref as a tool error and logs nothing', async () => {
    const repo = makeFixtureRepo();
    const res = await handler({ path: repo, base: 'nope', head: 'HEAD' });
    expect(res.isError).toBe(true);
  });
});
