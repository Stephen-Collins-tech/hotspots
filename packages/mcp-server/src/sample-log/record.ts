import { execa } from 'execa';
import { existsSync, realpathSync, statSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { openSampleLog } from './db.js';
import type { SampleTool } from '../types.js';

/** Resolve a user-supplied path to an absolute, symlink-free path. */
export function absPath(p: string): string {
  const abs = resolve(p);
  return existsSync(abs) ? realpathSync(abs) : abs;
}

/**
 * The repo a path belongs to: its git toplevel, or the path itself (its directory, if it is
 * a file) when git cannot tell. The sample log lives in `<repo>/.hotspots/`.
 */
export async function resolveRepoRoot(p: string): Promise<string> {
  const abs = absPath(p);
  const dir = existsSync(abs) && statSync(abs).isDirectory() ? abs : dirname(abs);
  const r = await execa('git', ['rev-parse', '--show-toplevel'], { cwd: dir, reject: false });
  return r.exitCode === 0 && r.stdout ? realpathSync(r.stdout.trim()) : dir;
}

export async function headSha(repoRoot: string): Promise<string | null> {
  const r = await execa('git', ['rev-parse', 'HEAD'], { cwd: repoRoot, reject: false });
  return r.exitCode === 0 ? r.stdout.trim() : null;
}

export interface RecordInput {
  tool: SampleTool;
  repoRoot: string;
  input: unknown;
  output: unknown;
  summary: unknown;
  exitCode?: number | null;
}

/** Insert exactly one sample row. Called once per successful tool invocation. */
export async function recordSample(r: RecordInput): Promise<void> {
  const sha = await headSha(r.repoRoot);
  const db = openSampleLog(r.repoRoot);
  try {
    db.prepare(
      `INSERT INTO samples
         (tool, taken_at, repo_path, commit_sha, input_json, output_json, exit_code, summary_json)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?)`
    ).run(
      r.tool,
      new Date().toISOString(),
      r.repoRoot,
      sha,
      JSON.stringify(r.input),
      JSON.stringify(r.output),
      r.exitCode ?? null,
      JSON.stringify(r.summary)
    );
  } finally {
    db.close();
  }
}
