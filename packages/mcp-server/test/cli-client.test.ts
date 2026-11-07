import { chmodSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { HotspotsVersionError, HotspotsNotFoundError, assertMinVersion, parseVersion, versionAtLeast, HotspotsParseError, binaryPath, runHotspots } from '../src/cli-client.js';

const saved = process.env.HOTSPOTS_PATH;
afterEach(() => {
  if (saved === undefined) delete process.env.HOTSPOTS_PATH;
  else process.env.HOTSPOTS_PATH = saved;
});

describe('cli-client', () => {
  it('defaults to hotspots on PATH and honors HOTSPOTS_PATH', () => {
    delete process.env.HOTSPOTS_PATH;
    expect(binaryPath()).toBe('hotspots');
    process.env.HOTSPOTS_PATH = '/x/hotspots';
    expect(binaryPath()).toBe('/x/hotspots');
  });

  it('throws HotspotsNotFoundError for a missing binary', async () => {
    process.env.HOTSPOTS_PATH = '/nonexistent/hotspots-bin';
    await expect(runHotspots(['--version'], { skipVersionCheck: true })).rejects.toBeInstanceOf(HotspotsNotFoundError);
  });

  it('parses JSON stdout for ok exit codes', async () => {
    process.env.HOTSPOTS_PATH = 'echo';
    await expect(runHotspots(['{"a":1}'], { skipVersionCheck: true })).resolves.toMatchObject({ json: { a: 1 }, exitCode: 0 });
  });

  it('throws HotspotsParseError on non-JSON stdout with an ok exit code', async () => {
    process.env.HOTSPOTS_PATH = 'echo';
    await expect(runHotspots(['not json'], { skipVersionCheck: true })).rejects.toBeInstanceOf(HotspotsParseError);
  });

  it('returns non-ok exit codes without json', async () => {
    process.env.HOTSPOTS_PATH = 'false';
    const r = await runHotspots([], { skipVersionCheck: true });
    expect(r.exitCode).toBe(1);
    expect(r.json).toBeUndefined();
  });

  it('compares versions against the 2.4.0 minimum', () => {
    expect(parseVersion('hotspots 2.4.0\n')).toEqual([2, 4, 0]);
    expect(versionAtLeast([2, 4, 0], '2.4.0')).toBe(true);
    expect(versionAtLeast([2, 10, 1], '2.4.0')).toBe(true);
    expect(versionAtLeast([3, 0, 0], '2.4.0')).toBe(true);
    expect(versionAtLeast([2, 3, 9], '2.4.0')).toBe(false);
  });

  it('rejects a binary that is too old or reports no version', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'fakebin-'));
    const old = join(dir, 'hotspots-old');
    writeFileSync(old, '#!/bin/sh\necho "hotspots 2.3.0"\n');
    chmodSync(old, 0o755);
    process.env.HOTSPOTS_PATH = old;
    await expect(assertMinVersion()).rejects.toBeInstanceOf(HotspotsVersionError);
    process.env.HOTSPOTS_PATH = 'echo';
    await expect(assertMinVersion()).rejects.toBeInstanceOf(HotspotsVersionError);
  });
});
