import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, realpathSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

/** Integration tests need a `hotspots` build that has `coordinate`; set HOTSPOTS_PATH to one. */
export const hasBinary = (() => {
  try {
    const out = execFileSync(process.env.HOTSPOTS_PATH || 'hotspots', ['coordinate', '--help'], { stdio: 'pipe' });
    return out.length > 0;
  } catch {
    return false;
  }
})();

export function git(cwd: string, ...args: string[]): string {
  return execFileSync('git', args, { cwd, encoding: 'utf8' }).trim();
}

/** Two-commit TS repo: src/b.ts regresses in complexity in the second commit. */
export function makeFixtureRepo(): string {
  const dir = realpathSync(mkdtempSync(join(tmpdir(), 'mcp-fixture-')));
  git(dir, 'init', '-q');
  git(dir, 'config', 'user.email', 't@example.com');
  git(dir, 'config', 'user.name', 'Tester');
  mkdirSync(join(dir, 'src'));
  writeFileSync(join(dir, 'src/a.ts'),
    'export function a(x: number) { if (x > 1) { return 1 } for (let i = 0; i < x; i++) { if (i % 2) { x-- } } return x }\n');
  writeFileSync(join(dir, 'src/b.ts'), 'export function b() { return 2 }\n');
  git(dir, 'add', '.');
  git(dir, 'commit', '-qm', 'one');
  writeFileSync(join(dir, 'src/b.ts'), 'export function b(y: number) { if (y) { if (y > 2) { return 3 } } return 2 }\n');
  git(dir, 'commit', '-qam', 'two');
  return dir;
}

export function logExists(repo: string): boolean {
  return existsSync(join(repo, '.hotspots', 'mcp-samples.db'));
}
