import { existsSync } from 'node:fs';
import { z } from 'zod';
import { runHotspots } from '../cli-client.js';
import { recordSample, resolveRepoRoot, absPath } from '../sample-log/record.js';
import { fail, ok, type AnalyzeSummary, type ToolResult } from '../types.js';

export const name = 'hotspots_analyze';

export const description =
  'Get the current risk triage for a repo or subtree (snapshot mode, JSON). Use it before or ' +
  'during work to see which functions are in fire / debt / watch / ok, ranked by activity risk. ' +
  'Set allFunctions for a flat list of every function instead of triage buckets. Optional blendChurn adds recent-churn-aware file ranking. Read-only; ' +
  'does not write a snapshot.';

export const shape = {
  path: z.string().describe('Repo root or subdirectory to analyze.'),
  minLrs: z.number().optional().describe('Minimum LRS threshold (--min-lrs).'),
  allFunctions: z.boolean().optional().describe('Flat array of all functions instead of triage buckets.'),
  touchMode: z.enum(['auto', 'per-function', 'file', 'hybrid', 'none']).optional()
    .describe('Touch metrics mode (--touch-mode). "none" is fastest.'),
  coldStart: z.boolean().optional().describe('Pass --cold-start.'),
  blendChurn: z.boolean().optional()
    .describe('Pass --blend-churn (CLI >= 2.4.0): re-rank file-level risk by a 50/50 blend of complexity-based file risk and recent churn. Adds churn_percentile and file_risk_score_blended to architecture.file_risk. Opt-in.'),
};

export const inputSchema = z.object(shape);
export type AnalyzeInput = z.infer<typeof inputSchema>;

/**
 * Max `lrs` reachable from the snapshot. The default triage only lists top functions for
 * non-ok buckets, so for triage output this is the max over those; allFunctions covers all.
 */
function maxLrs(output: any): number {
  let max = 0;
  const walk = (v: any) => {
    if (Array.isArray(v)) v.forEach(walk);
    else if (v && typeof v === 'object') {
      if (typeof v.lrs === 'number' && v.lrs > max) max = v.lrs;
      Object.values(v).forEach(walk);
    }
  };
  walk(output.triage ?? output.functions);
  return max;
}

/**
 * The CLI triage buckets are fire/debt/watch/ok. They are stored under the spec's
 * critical/high/watch/ok field names (fire -> critical, debt -> high).
 */
export function summarize(output: any): AnalyzeSummary {
  const t = output?.triage ?? {};
  return {
    criticalCount: t.fire?.count ?? 0,
    highCount: t.debt?.count ?? 0,
    watchCount: t.watch?.count ?? 0,
    okCount: t.ok?.count ?? 0,
    maxLrs: maxLrs(output),
  };
}

export async function handler(raw: unknown): Promise<ToolResult> {
  const parsed = inputSchema.safeParse(raw);
  if (!parsed.success) return fail(parsed.error.issues.map((i) => i.message).join('; '));
  const input = parsed.data;

  const path = absPath(input.path);
  if (!existsSync(path)) return fail(`Path does not exist: ${path}`);
  // --no-persist: snapshots are immutable per commit, so re-sampling the same commit would
  // otherwise fail; the sample log is this tool's record, not .hotspots/snapshots.
  const args = ['analyze', path, '--mode', 'snapshot', '--format', 'json', '--no-persist'];
  if (input.minLrs !== undefined) args.push('--min-lrs', String(input.minLrs));
  if (input.allFunctions) args.push('--all-functions');
  if (input.touchMode) args.push('--touch-mode', input.touchMode);
  if (input.coldStart) args.push('--cold-start');
  if (input.blendChurn) args.push('--blend-churn');

  const res = await runHotspots(args, { cwd: path });
  if (res.json === undefined) {
    return fail(`hotspots analyze exited ${res.exitCode}: ${res.stderr.trim()}`);
  }
  const summary = input.allFunctions
    ? { ...summarize({}), maxLrs: maxLrs(res.json) }
    : summarize(res.json);
  await recordSample({
    tool: 'analyze',
    repoRoot: await resolveRepoRoot(path),
    input,
    output: res.json,
    summary,
  });
  return ok(input.blendChurn ? withBlendedRanking(res.json) : res.json);
}

/**
 * With --blend-churn the per-file fields live deep in architecture.file_risk. Surface the
 * ranked files at the top level so a client does not have to dig; the CLI JSON is untouched.
 */
export function withBlendedRanking(output: any): unknown {
  const rows: any[] = output?.architecture?.file_risk ?? [];
  const blended = rows
    .filter((r) => typeof r.file_risk_score_blended === 'number')
    .sort((a, b) => b.file_risk_score_blended - a.file_risk_score_blended)
    .slice(0, 10)
    .map((r) => ({
      file: r.file,
      churn_percentile: r.churn_percentile,
      file_risk_score: r.file_risk_score,
      file_risk_score_blended: r.file_risk_score_blended,
    }));
  return blended.length ? { ...output, blended_file_ranking: blended } : output;
}
