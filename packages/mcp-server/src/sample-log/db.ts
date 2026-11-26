import Database from 'better-sqlite3';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

/** Ordered migrations; index + 1 is the `user_version` each one brings the DB to. */
const MIGRATIONS: string[] = [
  readFileSync(new URL('./schema.sql', import.meta.url), 'utf8'),
];

export const SCHEMA_VERSION = MIGRATIONS.length;

export type SampleDb = Database.Database;

export function samplesDbPath(repoRoot: string): string {
  return join(repoRoot, '.hotspots', 'mcp-samples.db');
}

/** Run any migrations the DB is behind on. Safe to call on every connect. */
export function migrate(db: SampleDb): void {
  const current = db.pragma('user_version', { simple: true }) as number;
  for (let v = current; v < MIGRATIONS.length; v++) {
    db.transaction(() => {
      db.exec(MIGRATIONS[v]);
      db.pragma(`user_version = ${v + 1}`);
    })();
  }
}

/**
 * Open (creating if needed) the sample log for a repo. The DB holds file paths and risk
 * data, so `.hotspots/.gitignore` is written to keep it out of commits even when the
 * target repo's own .gitignore does not cover `.hotspots/`.
 */
export function openSampleLog(repoRoot: string): SampleDb {
  const file = samplesDbPath(repoRoot);
  mkdirSync(dirname(file), { recursive: true });
  const ignore = join(dirname(file), '.gitignore');
  if (!existsSync(ignore)) {
    writeFileSync(ignore, 'mcp-samples.db*\n');
  }
  const db = new Database(file);
  db.pragma('journal_mode = WAL');
  migrate(db);
  return db;
}

/** Open an existing sample log read-only-ish; returns undefined if the repo has none yet. */
export function openExistingSampleLog(repoRoot: string): SampleDb | undefined {
  if (!existsSync(samplesDbPath(repoRoot))) return undefined;
  return openSampleLog(repoRoot);
}
