import { z } from 'zod';
import { openExistingSampleLog } from '../sample-log/db.js';
import { absPath, resolveRepoRoot } from '../sample-log/record.js';
import { fail, ok, type SampleRow, type ToolResult } from '../types.js';

export const historyName = 'hotspots_sample_history';
export const trendsName = 'hotspots_sample_trends';

export const historyDescription =
  'List recent entries from the local sample log (every hotspots_coordinate / analyze / ' +
  'policy_check call made through this server for a repo), newest first. Use it to see what ' +
  'was already sampled instead of re-running analysis. Reads the log only; never runs the CLI.';

export const trendsDescription =
  'Is risk rising or falling? Computes a trend over the most recent samples already in the ' +
  'sample log: riskTrend from analyze samples (max LRS, or one file\'s LRS) and ownershipTrend ' +
  'from coordinate samples. Reports insufficient-data with fewer than 2 samples. Never runs the CLI.';

export const historyShape = {
  tool: z.enum(['coordinate', 'analyze', 'policy_check']).optional().describe('Filter by producing tool.'),
  repoPath: z.string().optional().describe('Repo to read the log of. Defaults to the server working directory.'),
  limit: z.number().int().min(1).max(500).optional().describe('Max rows (default 20, max 500).'),
  since: z.string().datetime({ offset: true }).optional().describe('ISO 8601 timestamp; only samples at/after it.'),
};
export const historySchema = z.object(historyShape);

export const trendsShape = {
  repoPath: z.string().describe('Repo whose sample log to read.'),
  file: z.string().optional().describe('Narrow to one file (as it appears in the sampled output).'),
  window: z.number().int().min(1).max(500).optional().describe('Most recent samples to consider (default 10).'),
};
export const trendsSchema = z.object(trendsShape);

async function logFor(repoPath: string) {
  return openExistingSampleLog(await resolveRepoRoot(absPath(repoPath)));
}

export async function historyHandler(raw: unknown): Promise<ToolResult> {
  const parsed = historySchema.safeParse(raw);
  if (!parsed.success) return fail(parsed.error.issues.map((i) => i.message).join('; '));
  const { tool, repoPath, limit = 20, since } = parsed.data;

  const root = await resolveRepoRoot(absPath(repoPath ?? '.'));
  const db = openExistingSampleLog(root);
  if (!db) return ok([]);
  try {
    const where = ['repo_path = ?'];
    const params: unknown[] = [root];
    if (tool) { where.push('tool = ?'); params.push(tool); }
    if (since) { where.push('taken_at >= ?'); params.push(new Date(since).toISOString()); }
    const rows = db
      .prepare(`SELECT * FROM samples WHERE ${where.join(' AND ')} ORDER BY taken_at DESC, id DESC LIMIT ?`)
      .all(...params, limit) as SampleRow[];
    return ok(rows);
  } finally {
    db.close();
  }
}

type Direction = 'rising' | 'falling' | 'flat' | 'insufficient-data';

/**
 * Least-squares slope of values against sample index; sign gives direction. Slopes smaller
 * than 1e-9 of the value scale count as flat. Not a forecast.
 */
export function direction(values: number[]): Direction {
  if (values.length < 2) return 'insufficient-data';
  const n = values.length;
  const meanX = (n - 1) / 2;
  const meanY = values.reduce((a, b) => a + b, 0) / n;
  let num = 0;
  let den = 0;
  values.forEach((y, x) => { num += (x - meanX) * (y - meanY); den += (x - meanX) ** 2; });
  const slope = num / den;
  const eps = 1e-9 * Math.max(1, Math.abs(meanY));
  return slope > eps ? 'rising' : slope < -eps ? 'falling' : 'flat';
}

/** LRS of `file` in an analyze output: max over its functions found in triage tops / flat list. */
function fileLrs(output: any, file: string): number | null {
  let max: number | null = null;
  const walk = (v: any) => {
    if (Array.isArray(v)) v.forEach(walk);
    else if (v && typeof v === 'object') {
      if (typeof v.lrs === 'number' && typeof v.file === 'string' &&
          (v.file === file || v.file.endsWith('/' + file.replace(/^\.\//, '')))) {
        max = Math.max(max ?? 0, v.lrs);
      }
      Object.values(v).forEach(walk);
    }
  };
  walk(output.triage ?? output.functions);
  return max;
}

export async function trendsHandler(raw: unknown): Promise<ToolResult> {
  const parsed = trendsSchema.safeParse(raw);
  if (!parsed.success) return fail(parsed.error.issues.map((i) => i.message).join('; '));
  const { repoPath, file, window = 10 } = parsed.data;

  const root = await resolveRepoRoot(absPath(repoPath));
  const db = openExistingSampleLog(root);
  const empty = { sampleCount: 0, firstSampleAt: null, lastSampleAt: null };
  if (!db) return ok(empty);
  let rows: SampleRow[];
  try {
    rows = db
      .prepare(`SELECT * FROM samples WHERE repo_path = ? AND tool IN ('analyze','coordinate')
                ORDER BY taken_at DESC, id DESC LIMIT ?`)
      .all(root, window) as SampleRow[];
  } finally {
    db.close();
  }
  rows.reverse(); // oldest -> newest
  if (rows.length === 0) return ok(empty);

  const out: Record<string, unknown> = {
    sampleCount: rows.length,
    firstSampleAt: rows[0].taken_at,
    lastSampleAt: rows[rows.length - 1].taken_at,
  };

  // Each trend only uses samples of its own tool: mixed sample types are never combined.
  const analyze = rows.filter((r) => r.tool === 'analyze');
  if (analyze.length > 0) {
    const values = analyze
      .map((r) => file ? fileLrs(JSON.parse(r.output_json), file) : (JSON.parse(r.summary_json).maxLrs as number))
      .filter((v): v is number => v !== null);
    out.riskTrend = { direction: direction(values), values };
  }

  const coord = rows.filter((r) => r.tool === 'coordinate');
  if (coord.length > 0) {
    // Repo-wide: share of files with concentrated ownership (up => concentrating).
    // Per file: author_entropy (down => concentrating); null where the file wasn't sampled.
    const raw = coord.map((r) => {
      if (!file) {
        const s = JSON.parse(r.summary_json);
        return s.fileCount ? s.concentratedCount / s.fileCount : null;
      }
      const o = (JSON.parse(r.output_json).coordinate?.ownership ?? []).find((x: any) => x.file === file);
      return o ? (o.author_entropy as number) : null;
    });
    const present = raw.filter((v): v is number => v !== null);
    const d = direction(present);
    const flip = file ? { rising: 'diffusing', falling: 'concentrating' } : { rising: 'concentrating', falling: 'diffusing' };
    out.ownershipTrend = {
      direction: d === 'rising' ? flip.rising : d === 'falling' ? flip.falling : d,
      values: raw,
    };
  }
  return ok(out);
}
