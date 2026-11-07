import { mkdtempSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { openSampleLog } from '../../src/sample-log/db.js';
import { direction, historyHandler, trendsHandler } from '../../src/tools/samples.js';

function seed(rows: { tool: string; at: string; summary: object; output?: object }[]): string {
  const repo = realpathSync(mkdtempSync(join(tmpdir(), 'mcp-seed-')));
  const db = openSampleLog(repo);
  const ins = db.prepare(
    `INSERT INTO samples (tool,taken_at,repo_path,input_json,output_json,summary_json) VALUES (?,?,?,?,?,?)`);
  for (const r of rows) ins.run(r.tool, r.at, repo, '{}', JSON.stringify(r.output ?? {}), JSON.stringify(r.summary));
  db.close();
  return repo;
}
const analyzeAt = (n: number, maxLrs: number, file?: string) => ({
  tool: 'analyze', at: `2026-01-0${n}T00:00:00.000Z`,
  summary: { criticalCount: 0, highCount: 0, watchCount: 1, okCount: 0, maxLrs },
  output: { triage: { watch: { count: 1, top: [{ file: file ?? 'src/x.ts', lrs: maxLrs }] } } },
});
const text = (r: any) => JSON.parse(r.content[0].text);

describe('direction', () => {
  it('classifies slopes', () => {
    expect(direction([1])).toBe('insufficient-data');
    expect(direction([1, 2, 4])).toBe('rising');
    expect(direction([4, 2, 1])).toBe('falling');
    expect(direction([3, 3, 3])).toBe('flat');
  });
});

describe('hotspots_sample_history', () => {
  const repo = seed([
    analyzeAt(1, 1), analyzeAt(2, 2),
    { tool: 'coordinate', at: '2026-01-03T00:00:00.000Z', summary: { fileCount: 1, concentratedCount: 0 } },
  ]);
  it('returns newest first and honors filters/limit', async () => {
    const all = text(await historyHandler({ repoPath: repo }));
    expect(all.map((r: any) => r.tool)).toEqual(['coordinate', 'analyze', 'analyze']);
    expect(text(await historyHandler({ repoPath: repo, tool: 'analyze', limit: 1 }))).toHaveLength(1);
    expect(text(await historyHandler({ repoPath: repo, since: '2026-01-02T00:00:00Z' }))).toHaveLength(2);
  });
  it('is empty (and creates nothing) for a repo with no log', async () => {
    const empty = realpathSync(mkdtempSync(join(tmpdir(), 'mcp-empty-')));
    expect(text(await historyHandler({ repoPath: empty }))).toEqual([]);
  });
  it('rejects limit above 500', async () => {
    expect((await historyHandler({ repoPath: repo, limit: 501 })).isError).toBe(true);
  });
});

describe('hotspots_sample_trends', () => {
  it('insufficient-data with 0 or 1 samples', async () => {
    const none = realpathSync(mkdtempSync(join(tmpdir(), 'mcp-none-')));
    expect(text(await trendsHandler({ repoPath: none })).sampleCount).toBe(0);
    const one = seed([analyzeAt(1, 5)]);
    const r = text(await trendsHandler({ repoPath: one }));
    expect(r.sampleCount).toBe(1);
    expect(r.riskTrend).toEqual({ direction: 'insufficient-data', values: [5] });
  });
  it('rising and falling risk', async () => {
    const up = text(await trendsHandler({ repoPath: seed([analyzeAt(1, 1), analyzeAt(2, 2), analyzeAt(3, 5)]) }));
    expect(up.riskTrend).toEqual({ direction: 'rising', values: [1, 2, 5] });
    const down = text(await trendsHandler({ repoPath: seed([analyzeAt(1, 9), analyzeAt(2, 4), analyzeAt(3, 1)]) }));
    expect(down.riskTrend.direction).toBe('falling');
  });
  it('respects window and per-file narrowing', async () => {
    const repo = seed([analyzeAt(1, 9), analyzeAt(2, 1), analyzeAt(3, 2), analyzeAt(4, 3)]);
    const w = text(await trendsHandler({ repoPath: repo, window: 3 }));
    expect(w.sampleCount).toBe(3);
    expect(w.riskTrend.values).toEqual([1, 2, 3]);
    const f = text(await trendsHandler({ repoPath: repo, file: 'src/x.ts', window: 3 }));
    expect(f.riskTrend.direction).toBe('rising');
    const miss = text(await trendsHandler({ repoPath: repo, file: 'src/none.ts' }));
    expect(miss.riskTrend).toEqual({ direction: 'insufficient-data', values: [] });
  });
  it('ownership trend from coordinate samples, kept separate from risk', async () => {
    const c = (n: number, k: number) => ({ tool: 'coordinate', at: `2026-01-0${n}T00:00:00.000Z`, summary: { fileCount: 4, concentratedCount: k } });
    const r = text(await trendsHandler({ repoPath: seed([c(1, 0), c(2, 2), c(3, 4)]) }));
    expect(r.riskTrend).toBeUndefined();
    expect(r.ownershipTrend.direction).toBe('concentrating');
    const d = text(await trendsHandler({ repoPath: seed([c(1, 4), c(2, 2), c(3, 0)]) }));
    expect(d.ownershipTrend.direction).toBe('diffusing');
  });
});
