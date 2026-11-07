import { readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { describe, expect, it } from 'vitest';
import Database from 'better-sqlite3';
import { handler, inputSchema, summarize } from '../../src/tools/coordinate.js';
import { hasBinary, makeFixtureRepo } from '../helpers.js';

describe('coordinate input validation', () => {
  it('accepts exactly one of files/staged/diff', () => {
    expect(inputSchema.safeParse({ files: ['a.ts'] }).success).toBe(true);
    expect(inputSchema.safeParse({ staged: true }).success).toBe(true);
    expect(inputSchema.safeParse({ diff: 'diff --git a/x b/x' }).success).toBe(true);
  });
  it('rejects zero or multiple', () => {
    expect(inputSchema.safeParse({}).success).toBe(false);
    expect(inputSchema.safeParse({ staged: false }).success).toBe(false);
    expect(inputSchema.safeParse({ files: ['a'], staged: true }).success).toBe(false);
    expect(inputSchema.safeParse({ files: ['a'], diff: 'x' }).success).toBe(false);
  });
  it('returns a tool error, not a throw', async () => {
    const r = await handler({});
    expect(r.isError).toBe(true);
    expect(r.content[0].text).toMatch(/exactly one/);
  });
  it('summarizes concentrated files', () => {
    const out = { coordinate: { ownership: [{ knowledge_mode: 'concentrated' }, { knowledge_mode: null }] } };
    expect(summarize(out)).toEqual({ fileCount: 2, concentratedCount: 1 });
  });
});

describe.skipIf(!hasBinary)('coordinate integration', () => {
  it('matches the CLI output and writes one sample row', async () => {
    const repo = makeFixtureRepo();
    const res = await handler({ path: repo, files: ['src/a.ts', 'src/b.ts'] });
    const cli = JSON.parse(execFileSync(process.env.HOTSPOTS_PATH || 'hotspots',
      ['coordinate', repo, '--files', 'src/a.ts,src/b.ts'], { cwd: repo, encoding: 'utf8' }));
    expect(JSON.parse(res.content[0].text)).toEqual(cli);
    expect(cli.schema_version).toBe(5);

    const db = new Database(`${repo}/.hotspots/mcp-samples.db`, { readonly: true });
    const rows = db.prepare('SELECT * FROM samples').all() as any[];
    expect(rows).toHaveLength(1);
    expect(rows[0].tool).toBe('coordinate');
    expect(JSON.parse(rows[0].summary_json)).toEqual({ fileCount: 2, concentratedCount: 0 });
    expect(rows[0].commit_sha).toMatch(/^[0-9a-f]{40}$/);
  });

  it('derives files from a diff on stdin', async () => {
    const repo = makeFixtureRepo();
    const diff = execFileSync('git', ['diff', 'HEAD~1', 'HEAD'], { cwd: repo, encoding: 'utf8' });
    const res = await handler({ path: repo, diff });
    expect(JSON.parse(res.content[0].text).coordinate.input_files).toEqual(['src/b.ts']);
  });
});
