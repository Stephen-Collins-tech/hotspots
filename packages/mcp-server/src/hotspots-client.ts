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

export interface RunOptions {
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
