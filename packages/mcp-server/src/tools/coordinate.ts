import { existsSync } from 'node:fs';
import { z } from 'zod';
import { runHotspots } from '../cli-client.js';
import { recordSample, resolveRepoRoot, absPath } from '../sample-log/record.js';
import { fail, ok, type CoordinateSummary, type ToolResult } from '../types.js';

export const name = 'hotspots_coordinate';

export const description =
  'Check ownership and entanglement risk for a set of files BEFORE you start editing them. ' +
  'Reports, per file, author count, author entropy, newcomer rate and whether knowledge is ' +
  '"concentrated" in few people. Provide exactly one of: files (explicit list), staged ' +
  '(use the git index), or diff (unified diff text). Read-only.';

export const shape = {
  path: z.string().optional().describe('Repo root. Defaults to the server working directory (the CLI default, ".").'),
  files: z.array(z.string()).min(1).optional().describe('Files to check (relative to path).'),
  staged: z.boolean().optional().describe('Use files staged in git (mutually exclusive with files/diff).'),
  diff: z.string().optional().describe('Unified diff text to derive the file set from (mutually exclusive with files/staged).'),
};

export const inputSchema = z.object(shape).refine(
  (v) => [v.files !== undefined, v.staged === true, v.diff !== undefined].filter(Boolean).length === 1,
  { message: 'Provide exactly one of: files, staged, diff.' }
);

export type CoordinateInput = z.infer<typeof inputSchema>;

export function summarize(output: any): CoordinateSummary {
  const ownership: any[] = output?.coordinate?.ownership ?? [];
  return {
    fileCount: ownership.length,
    concentratedCount: ownership.filter((o) => o.knowledge_mode === 'concentrated').length,
  };
}

export async function handler(raw: unknown): Promise<ToolResult> {
  const parsed = inputSchema.safeParse(raw);
  if (!parsed.success) return fail(parsed.error.issues.map((i) => i.message).join('; '));
  const input = parsed.data;

  const path = absPath(input.path ?? '.');
  if (!existsSync(path)) return fail(`Path does not exist: ${path}`);
  const args = ['coordinate', path];
  if (input.files) args.push('--files', input.files.join(','));
  if (input.staged) args.push('--staged');
  if (input.diff !== undefined) args.push('--diff');

  const res = await runHotspots(args, { cwd: path, input: input.diff });
  if (res.json === undefined) {
    return fail(`hotspots coordinate exited ${res.exitCode}: ${res.stderr.trim()}`);
  }
  await recordSample({
    tool: 'coordinate',
    repoRoot: await resolveRepoRoot(path),
    input,
    output: res.json,
    summary: summarize(res.json),
  });
  return ok(res.json);
}
