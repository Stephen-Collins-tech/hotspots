import { existsSync } from 'node:fs';
import { z } from 'zod';
import { runHotspots } from '../hotspots-client.js';
import { recordSample, resolveRepoRoot, absPath } from '../sample-log/record.js';
import { fail, ok, type PolicyCheckSummary, type ToolResult } from '../types.js';

export const name = 'hotspots_policy_check';

export const description =
  'Evaluate whether the changes between two git refs would fail the repo policy gate. Use it ' +
  'before committing or opening a PR. Returns the diff JSON plus exitCode/exitMeaning: ' +
  'exit 1 means blocking violations (critical-introduction, excessive-risk-regression); ' +
  'warnings (watch-threshold, attention-threshold, rapid-growth, suppression-missing-reason, ' +
  'net-repo-regression) do not block. Set autoAnalyze if snapshots for the refs may be missing. Read-only.';

export const shape = {
  path: z.string().describe('Repo root.'),
  base: z.string().describe('Base git ref (branch, tag, SHA, HEAD~N).'),
  head: z.string().describe('Head git ref.'),
  top: z.number().int().positive().optional().describe('Limit output to top N changed functions (--top).'),
  autoAnalyze: z.boolean().optional().describe('Generate missing snapshots via git worktrees (--auto-analyze).'),
};

export const inputSchema = z.object(shape);
export type PolicyCheckInput = z.infer<typeof inputSchema>;

// docs/REFERENCE.md "Exit codes" (diff-relevant rows).
export const EXIT_MEANINGS: Record<number, string> = {
  0: 'Success (or warnings only)',
  1: 'Blocking policy failure',
  2: 'Auto-analysis failed (--auto-analyze)',
  3: 'Snapshot missing (re-run with autoAnalyze, or analyze both refs first)',
};

export function summarize(diff: any): PolicyCheckSummary {
  const policy = diff?.delta?.policy;
  return {
    blockingViolations: policy?.failed?.length ?? 0,
    warnings: policy?.warnings?.length ?? 0,
  };
}

export async function handler(raw: unknown): Promise<ToolResult> {
  const parsed = inputSchema.safeParse(raw);
  if (!parsed.success) return fail(parsed.error.issues.map((i) => i.message).join('; '));
  const input = parsed.data;

  const path = absPath(input.path);
  if (!existsSync(path)) return fail(`Path does not exist: ${path}`);
  const args = ['diff', input.base, input.head, '--policy', '--format', 'json'];
  if (input.top !== undefined) args.push('--top', String(input.top));
  if (input.autoAnalyze) args.push('--auto-analyze');

  // Exit 1 is a meaningful policy failure carrying JSON; 2/3 are execution states without it.
  const res = await runHotspots(args, { cwd: path, okExitCodes: [0, 1] }).catch((e) => {
    // exit 1 without JSON (e.g. unresolvable ref) surfaces as a parse error; report as such.
    if (e?.name === 'HotspotsParseError') return { json: undefined, stdout: e.output, stderr: e.output, exitCode: 1 };
    throw e;
  });
  const exitMeaning = EXIT_MEANINGS[res.exitCode] ?? `Error (exit ${res.exitCode})`;
  if (res.json === undefined) {
    return fail(`hotspots diff exited ${res.exitCode} (${exitMeaning}): ${res.stderr.trim()}`);
  }
  const result = { exitCode: res.exitCode, exitMeaning, diff: res.json };
  await recordSample({
    tool: 'policy_check',
    repoRoot: await resolveRepoRoot(path),
    input,
    output: result,
    summary: summarize(res.json),
    exitCode: res.exitCode,
  });
  return ok(result);
}
