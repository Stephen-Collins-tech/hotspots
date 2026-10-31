import { describe, expect, it } from 'vitest';
import { buildReason, handler, inputSchema, rankCandidates } from '../../src/tools/review-brief.js';
import { hasBinary, makeFixtureRepo } from '../helpers.js';

const output = {
  architecture: { file_risk: [
    { file: 'a.ts', max_cc: 4, function_count: 1, file_risk_score: 3, churn_percentile: 0, file_risk_score_blended: 0.5 },
    { file: 'b.ts', max_cc: 3, function_count: 1, file_risk_score: 2.3, churn_percentile: 1, file_risk_score_blended: 0.9 },
  ] },
};

describe('review brief logic', () => {
  it('validates limit', () => {
    expect(inputSchema.safeParse({ path: '.', limit: 11 }).success).toBe(false);
    expect(inputSchema.safeParse({ path: '.' }).success).toBe(true);
  });
  it('ranks by blended score by default and by file risk when blend is off', () => {
    expect(rankCandidates(output, true, 5).map((c) => c.file)).toEqual(['b.ts', 'a.ts']);
    expect(rankCandidates(output, false, 5).map((c) => c.file)).toEqual(['a.ts', 'b.ts']);
    expect(rankCandidates(output, true, 1)).toHaveLength(1);
  });
  it('writes a plain, non-prescriptive reason', () => {
    const r = buildReason({ file: 'b.ts', maxCc: 3, churnPercentile: 1 }, { name: 'b' }, { author_count: 1 });
    expect(r).toContain('complexity 3');
    expect(r).toContain('most recently changed');
    expect(r).toContain('single author');
    expect(r).not.toMatch(/refactor|should|extract|split/i);
  });
});

describe.skipIf(!hasBinary)('review brief integration', () => {
  it('returns a ranked brief with reasons from a real binary', async () => {
    const repo = makeFixtureRepo();
    const r = await handler({ path: repo, limit: 2 });
    expect(r.isError).toBeUndefined();
    const body = JSON.parse(r.content[0].text);
    expect(body.blendChurn).toBe(true);
    expect(body.brief).toHaveLength(2);
    expect(body.brief[0].rank).toBe(1);
    expect(body.brief.map((b: any) => b.file).sort()).toEqual(['src/a.ts', 'src/b.ts']);
    for (const b of body.brief) expect(b.reason).toMatch(/single author/);
  });
});
