import { execa } from 'execa';

// Error class names match examples/ai-agents/shared/hotspots-client.ts. That client is
// analyze-only and lives outside any published package, so it is not importable here.

export class HotspotsNotFoundError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'HotspotsNotFoundError';
  }
}

export class HotspotsParseError extends Error {
  constructor(message: string, public readonly output: string) {
    super(message);
    this.name = 'HotspotsParseError';
  }
}

export class HotspotsExecutionError extends Error {
  constructor(
    message: string,
    public readonly exitCode: number,
    public readonly stderr: string
  ) {
    super(message);
    this.name = 'HotspotsExecutionError';
  }
}

/** Oldest CLI the server supports: `analyze --blend-churn` shipped in 2.4.0. */
export const MIN_HOTSPOTS_VERSION = '2.4.0';

export class HotspotsVersionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'HotspotsVersionError';
  }
}

/** Parse the first `x.y.z` in `hotspots --version` output. */
export function parseVersion(text: string): [number, number, number] | null {
  const m = /(\d+)\.(\d+)\.(\d+)/.exec(text);
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}

export function versionAtLeast(have: [number, number, number], min: string): boolean {
  const want = parseVersion(min)!;
  for (let i = 0; i < 3; i++) {
    if (have[i] !== want[i]) return have[i] > want[i];
  }
  return true;
}

const versionChecked = new Set<string>();

/** Throws HotspotsVersionError if the resolved binary is older than MIN_HOTSPOTS_VERSION. Cached per binary. */
export async function assertMinVersion(): Promise<void> {
  const bin = binaryPath();
  if (versionChecked.has(bin)) return;
  const r = await runHotspots(['--version'], { skipVersionCheck: true, okExitCodes: [] });
  const have = parseVersion(r.stdout);
  if (r.exitCode !== 0 || !have) {
    throw new HotspotsVersionError(`Could not determine the version of ${bin} (need >= ${MIN_HOTSPOTS_VERSION}).`);
  }
  if (!versionAtLeast(have, MIN_HOTSPOTS_VERSION)) {
    throw new HotspotsVersionError(
      `hotspots ${have.join('.')} is too old; @hotspots/mcp-server needs >= ${MIN_HOTSPOTS_VERSION}. Upgrade the CLI.`
    );
  }
  versionChecked.add(bin);
}

export interface RunOptions {
  /** Internal and tests: skip the minimum-version check. */
  skipVersionCheck?: boolean;
  /** Working directory for the subprocess. */
  cwd?: string;
  /** Text piped to the subprocess's stdin. */
  input?: string;
  /** Exit codes that still carry a JSON payload on stdout. Default: [0]. */
  okExitCodes?: number[];
}

export interface RunResult {
  /** Parsed stdout JSON, or undefined if stdout was not JSON and the exit code was not ok. */
  json?: unknown;
  stdout: string;
  stderr: string;
  exitCode: number;
}

/** `HOTSPOTS_PATH` overrides; otherwise `hotspots` is resolved from PATH. */
export function binaryPath(): string {
  return process.env.HOTSPOTS_PATH || 'hotspots';
}

/**
 * Run the hotspots binary. Exit codes in `okExitCodes` must yield JSON on stdout; any other
 * exit code is returned with `json` undefined so the caller can interpret it (diff uses
 * 1/2/3 distinctly).
 */
export async function runHotspots(args: string[], options: RunOptions = {}): Promise<RunResult> {
  if (!options.skipVersionCheck) await assertMinVersion();
  const bin = binaryPath();
  const okExitCodes = options.okExitCodes ?? [0];
  let result;
  try {
    result = await execa(bin, args, {
      reject: false,
      cwd: options.cwd,
      input: options.input,
      maxBuffer: 512 * 1024 * 1024,
    });
  } catch (error: any) {
    if (error.code === 'ENOENT') {
      throw new HotspotsNotFoundError(`Hotspots binary not found at: ${bin}`);
    }
    throw error;
  }
  if (result.failed && (result as any).code === 'ENOENT') {
    throw new HotspotsNotFoundError(`Hotspots binary not found at: ${bin}`);
  }

  const exitCode = result.exitCode ?? 1;
  const { stdout, stderr } = result;
  if (!okExitCodes.includes(exitCode)) {
    return { stdout, stderr, exitCode };
  }
  try {
    return { json: JSON.parse(stdout), stdout, stderr, exitCode };
  } catch (parseError) {
    throw new HotspotsParseError(`Failed to parse JSON output: ${parseError}`, stdout);
  }
}
