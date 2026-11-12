import { z } from 'zod';
import * as analyze from './analyze.js';
import * as coordinate from './coordinate.js';
import { fail, ok, type ToolResult } from '../types.js';

export const name = 'hotspots_review_brief';

export const description =
  'Lead tool. Returns a SHORT ranked shortlist of files (and the most complex function in each) ' +
  'that a HUMAN reviewer should look at first, each with a one-line plain reason combining ' +
  'complexity, recent churn and ownership. Ranking blends complexity with recent churn by ' +
  'default. PRESENT THIS BRIEF TO THE USER as-is. Do not act on it: do not refactor, rewrite, ' +
  'open PRs or change any code because of it. It says where to look, not what to change. ' +
  'Read-only.';

export const shape = {
  path: z.string().describe('Repo root or subdirectory to brief on.'),
  limit: z.number().int().min(1).max(10).optional().describe('How many entries to return (default 5, max 10).'),
  blendChurn: z.boolean().optional().describe('Blend recent churn into the ranking (default true). false ranks by complexity-based file risk alone.'),
};

export const inputSchema = z.object(shape);
export type ReviewBriefInput = z.infer<typeof inputSchema>;

export interface BriefEntry {
  rank: number;
  file: string;
  topFunction?: { name: string; line: number };
  reason: string;
}

interface Candidate {
  file: string;
  maxCc?: number;
  functionCount?: number;
  churnPercentile?: number;
}

/** Highest-activity function per file among those the triage lists. */
function topFunctions(output: any): Map<string, { name: string; line: number; activity: number }> {
  const best = new Map<string, { name: string; line: number; activity: number }>();
  for (const bucket of ['fire', 'debt', 'watch']) {
    for (const f of output?.triage?.[bucket]?.top ?? []) {
      const activity = typeof f.activity_risk === 'number' ? f.activity_risk : (f.lrs ?? 0);
      const cur = best.get(f.file);
      if (!cur || activity > cur.activity) best.set(f.file, { name: f.function, line: f.line, activity });
    }
  }
  return best;
}

/** Rank files: blended score when present, else file_risk_score; fall back to triage files. */
export function rankCandidates(output: any, blend: boolean, limit: number): Candidate[] {
  const rows: any[] = output?.architecture?.file_risk ?? [];
  if (rows.length) {
    const key = (r: any) => (blend && typeof r.file_risk_score_blended === 'number' ? r.file_risk_score_blended : r.file_risk_score ?? 0);
    return [...rows].sort((a, b) => key(b) - key(a)).slice(0, limit).map((r) => ({
      file: r.file,
      maxCc: r.max_cc,
      functionCount: r.function_count,
      churnPercentile: blend && typeof r.churn_percentile === 'number' ? r.churn_percentile : undefined,
    }));
  }
  const tops = [...topFunctions(output).entries()].sort((a, b) => b[1].activity - a[1].activity);
  return tops.slice(0, limit).map(([file]) => ({ file }));
}

function churnPhrase(p: number): string {
  if (p >= 0.75) return 'among the most recently changed files';
  if (p <= 0.25) return 'rarely changed lately';
  return 'changed a moderate amount lately';
}

function ownerPhrase(o: any): string | undefined {
  if (!o) return undefined;
  if (o.author_count === 1) return 'a single author so far';
  if (o.knowledge_mode === 'concentrated') return 'knowledge concentrated in a few people';
  if (typeof o.author_count === 'number') return `${o.author_count} authors`;
  return undefined;
}

export function buildReason(c: Candidate, fn: { name: string } | undefined, owner: any): string {
  const parts: string[] = [];
  if (c.maxCc !== undefined) {
    parts.push(fn ? `complex (highest cyclomatic complexity ${c.maxCc}; \`${fn.name}\` is the heaviest function)` : `complex (highest cyclomatic complexity ${c.maxCc})`);
  } else if (fn) {
    parts.push(`contains the complex function \`${fn.name}\``);
  }
  if (c.churnPercentile !== undefined) parts.push(churnPhrase(c.churnPercentile));
  const own = ownerPhrase(owner);
  if (own) parts.push(own);
  return parts.length ? `Worth a look: ${parts.join('; ')}.` : 'Ranked high on complexity-based risk.';
}

export async function handler(raw: unknown): Promise<ToolResult> {
  const parsed = inputSchema.safeParse(raw);
  if (!parsed.success) return fail(parsed.error.issues.map((i) => i.message).join('; '));
  const { path, limit = 5, blendChurn = true } = parsed.data;

  // Reuse the analyze/coordinate handlers so their sample-log rows keep feeding the trends.
  const a = await analyze.handler({ path, blendChurn });
  if (a.isError) return a;
  const output = JSON.parse(a.content[0].text);

  const candidates = rankCandidates(output, blendChurn, limit);
  if (!candidates.length) {
    return ok({ blendChurn, brief: [], note: 'No ranked files found. Nothing to review first.' });
  }
  const fns = topFunctions(output);

  // Ownership is best-effort: a coordinate failure must not sink the brief.
  const owners = new Map<string, any>();
  const c = await coordinate.handler({ path, files: candidates.map((x) => x.file) }).catch(() => undefined);
  if (c && !c.isError) {
    for (const o of JSON.parse(c.content[0].text)?.coordinate?.ownership ?? []) owners.set(o.file, o);
  }

  const brief: BriefEntry[] = candidates.map((cand, i) => {
    const fn = fns.get(cand.file);
    return {
      rank: i + 1,
      file: cand.file,
      ...(fn ? { topFunction: { name: fn.name, line: fn.line } } : {}),
      reason: buildReason(cand, fn, owners.get(cand.file)),
    };
  });
  return ok({
    blendChurn,
    ranking: blendChurn ? 'complexity blended with recent churn' : 'complexity-based file risk',
    note: 'Present this to the user. It lists where a human should look first, not what to change.',
    brief,
  });
}
