import { execa } from 'execa';
import type { HotspotsOutput } from '@hotspots/types';
import { isHotspotsOutput } from '@hotspots/types';

/**
 * Error thrown when hotspots binary cannot be found
 */
export class HotspotsNotFoundError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'HotspotsNotFoundError';
  }
}

/**
 * Error thrown when hotspots JSON output cannot be parsed
 */
export class HotspotsParseError extends Error {
  constructor(message: string, public readonly output: string) {
    super(message);
    this.name = 'HotspotsParseError';
  }
}

/**
 * Error thrown when hotspots execution fails
 */
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

export interface AnalyzeOptions {
  path: string;
  mode?: 'snapshot' | 'delta';
  minLrs?: number;
  config?: string;
  format?: 'json' | 'text' | 'html';
  policies?: boolean;
}

/**
 * Client for running Hotspots complexity analysis
 *
 * @example
 * ```typescript
 * const client = new HotspotsClient();
 * const result = await client.analyze({ path: 'src/', mode: 'snapshot' });
 * console.log(`Analyzed ${result.functions.length} functions`);
 * ```
 */
export class HotspotsClient {
  private binaryPath: string;

  /**
   * Create a new HotspotsClient
   *
   * @param binaryPath - Path to hotspots binary (default: 'hotspots' from PATH)
   */
  constructor(binaryPath: string = 'hotspots') {
    this.binaryPath = binaryPath;
  }

  /**
   * Run hotspots analyze command
   *
   * @param options - Analysis options
   * @returns Parsed Hotspots output
   * @throws {HotspotsNotFoundError} If binary not found
   * @throws {HotspotsExecutionError} If analysis fails
   * @throws {HotspotsParseError} If output cannot be parsed
   */
  async analyze(options: AnalyzeOptions): Promise<HotspotsOutput> {
    const args: string[] = ['analyze', '--format', options.format || 'json'];

    if (options.mode) {
      args.push('--mode', options.mode);
    }

    if (options.minLrs !== undefined) {
      args.push('--min-lrs', options.minLrs.toString());
    }

    if (options.config) {
      args.push('--config', options.config);
    }

    if (options.policies) {
      args.push('--policies');
    }

    args.push(options.path);

    try {
      const { stdout, stderr, exitCode } = await execa(this.binaryPath, args, {
        reject: false,
      });

      if (exitCode !== 0) {
        throw new HotspotsExecutionError(
          `Hotspots exited with code ${exitCode}`,
          exitCode,
          stderr
        );
      }

      let output: unknown;
      try {
        output = JSON.parse(stdout);
      } catch (parseError) {
        throw new HotspotsParseError(
          `Failed to parse JSON output: ${parseError}`,
          stdout
        );
      }

      if (!isHotspotsOutput(output)) {
        throw new HotspotsParseError(
          'Output does not match expected schema',
          stdout
        );
      }

      return output;
    } catch (error: any) {
      if (error.code === 'ENOENT') {
        throw new HotspotsNotFoundError(
          `Hotspots binary not found at: ${this.binaryPath}`
        );
      }
      throw error;
    }
  }

  /**
   * Analyze a single file
   *
   * @param filePath - Path to file
   * @param mode - Analysis mode
   * @returns Parsed Hotspots output
   */
  async analyzeFile(
    filePath: string,
    mode?: 'snapshot' | 'delta'
  ): Promise<HotspotsOutput> {
    return this.analyze({ path: filePath, mode });
  }

  /**
   * Analyze a directory
   *
   * @param dirPath - Path to directory
   * @param mode - Analysis mode
   * @returns Parsed Hotspots output
   */
  async analyzeDirectory(
    dirPath: string,
    mode?: 'snapshot' | 'delta'
  ): Promise<HotspotsOutput> {
    return this.analyze({ path: dirPath, mode });
  }
}
