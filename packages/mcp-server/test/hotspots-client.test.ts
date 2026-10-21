import { afterEach, describe, expect, it } from 'vitest';
import { HotspotsNotFoundError, HotspotsParseError, binaryPath, runHotspots } from '../src/hotspots-client.js';

const saved = process.env.HOTSPOTS_PATH;
afterEach(() => {
  if (saved === undefined) delete process.env.HOTSPOTS_PATH;
  else process.env.HOTSPOTS_PATH = saved;
});

describe('hotspots-client', () => {
  it('defaults to hotspots on PATH and honors HOTSPOTS_PATH', () => {
    delete process.env.HOTSPOTS_PATH;
    expect(binaryPath()).toBe('hotspots');
    process.env.HOTSPOTS_PATH = '/x/hotspots';
    expect(binaryPath()).toBe('/x/hotspots');
  });

  it('throws HotspotsNotFoundError for a missing binary', async () => {
    process.env.HOTSPOTS_PATH = '/nonexistent/hotspots-bin';
    await expect(runHotspots(['--version'])).rejects.toBeInstanceOf(HotspotsNotFoundError);
  });

  it('parses JSON stdout for ok exit codes', async () => {
    process.env.HOTSPOTS_PATH = 'echo';
    await expect(runHotspots(['{"a":1}'])).resolves.toMatchObject({ json: { a: 1 }, exitCode: 0 });
  });

  it('throws HotspotsParseError on non-JSON stdout with an ok exit code', async () => {
    process.env.HOTSPOTS_PATH = 'echo';
    await expect(runHotspots(['not json'])).rejects.toBeInstanceOf(HotspotsParseError);
  });

  it('returns non-ok exit codes without json', async () => {
    process.env.HOTSPOTS_PATH = 'false';
    const r = await runHotspots([]);
    expect(r.exitCode).toBe(1);
    expect(r.json).toBeUndefined();
  });
});
