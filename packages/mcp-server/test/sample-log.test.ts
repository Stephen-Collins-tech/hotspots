import Database from 'better-sqlite3';
import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { SCHEMA_VERSION, migrate, openSampleLog, samplesDbPath } from '../src/sample-log/db.js';

describe('sample log schema', () => {
  it('migrates an empty in-memory DB to the current version, idempotently', () => {
    const db = new Database(':memory:');
    expect(db.pragma('user_version', { simple: true })).toBe(0);
    migrate(db);
    migrate(db);
    expect(db.pragma('user_version', { simple: true })).toBe(SCHEMA_VERSION);
    const cols = db.prepare('PRAGMA table_info(samples)').all().map((c: any) => c.name);
    expect(cols).toEqual(['id', 'tool', 'taken_at', 'repo_path', 'commit_sha', 'input_json', 'output_json', 'exit_code', 'summary_json']);
  });

  it('rejects unknown tool values', () => {
    const db = new Database(':memory:');
    migrate(db);
    expect(() => db.prepare(
      `INSERT INTO samples (tool,taken_at,repo_path,input_json,output_json,summary_json) VALUES ('bogus','t','r','{}','{}','{}')`
    ).run()).toThrow();
  });

  it('creates .hotspots/mcp-samples.db plus a self-ignoring .gitignore', () => {
    const repo = mkdtempSync(join(tmpdir(), 'mcp-log-'));
    openSampleLog(repo).close();
    expect(samplesDbPath(repo)).toMatch(/\.hotspots\/mcp-samples\.db$/);
    expect(readFileSync(join(repo, '.hotspots', '.gitignore'), 'utf8')).toContain('mcp-samples.db');
  });
});
