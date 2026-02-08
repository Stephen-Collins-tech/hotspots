import { execa } from 'execa';
import * as fs from 'fs';
import * as path from 'path';

export interface AnalyzeInput {
  path: string;
  mode?: 'snapshot' | 'delta';
  minLrs?: number;
  config?: string;
}

export interface AnalyzeResult {
  success: boolean;
  output?: any;
  summary?: string;
  error?: string;
}

/**
 * Find the hotspots binary on the system
 */
async function findHotspotsBinary(): Promise<string> {
  // Check if HOTSPOTS_PATH env var is set
  if (process.env.HOTSPOTS_PATH && fs.existsSync(process.env.HOTSPOTS_PATH)) {
    return process.env.HOTSPOTS_PATH;
  }

  try {
    // Use 'where' on Windows, 'which' on Unix/Mac
    const command = process.platform === 'win32' ? 'where' : 'which';
    const { stdout } = await execa(command, ['hotspots']);
    return stdout.trim().split('\n')[0]; // Windows 'where' may return multiple paths
  } catch {
    throw new Error(
      'hotspots binary not found in PATH. Please install hotspots or add it to your PATH.'
    );
  }
}

/**
 * Execute hotspots analyze command
 */
export async function analyze(input: AnalyzeInput): Promise<AnalyzeResult> {
  try {
    // Validate input
    if (!input.path) {
      return {
        success: false,
        error: 'path parameter is required',
      };
    }

    // Check if path exists
    const targetPath = path.resolve(input.path);
    if (!fs.existsSync(targetPath)) {
      return {
        success: false,
        error: `Path does not exist: ${input.path}`,
      };
    }

    // Find hotspots binary
    const hotspotsPath = await findHotspotsBinary();

    // Build command arguments
    const args: string[] = ['analyze', '--format', 'json'];

    if (input.mode) {
      args.push('--mode', input.mode);
    }

    if (input.minLrs !== undefined) {
      args.push('--min-lrs', input.minLrs.toString());
    }

    if (input.config) {
      args.push('--config', input.config);
    }

    args.push(targetPath);

    // Execute hotspots
    const result = await execa(hotspotsPath, args, {
      timeout: 30000,
      reject: false,
    });

    // Check exit code for errors (more reliable than parsing stderr)
    if (result.exitCode !== 0) {
      return {
        success: false,
        error: result.stderr || result.stdout || 'Analysis failed with non-zero exit code',
      };
    }

    const { stdout } = result;

    // Parse JSON output
    let output;
    try {
      output = JSON.parse(stdout);
    } catch (parseError) {
      return {
        success: false,
        error: `Failed to parse JSON output: ${parseError}`,
      };
    }

    // Generate summary
    const summary = generateSummary(output);

    return {
      success: true,
      output,
      summary,
    };
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

/**
 * Generate a human-readable summary of analysis results
 */
function generateSummary(output: any): string {
  const lines: string[] = [];

  lines.push('# Hotspots Analysis Results\n');

  // Handle both snapshot/delta mode (object) and plain array output
  const isStructuredOutput = output && typeof output === 'object' && !Array.isArray(output);
  const functions = isStructuredOutput ? output.functions : output;

  // Commit info (only in structured output)
  if (isStructuredOutput && output.commit) {
    lines.push(`**Commit:** ${output.commit.sha.substring(0, 8)}`);
    if (output.commit.branch) {
      lines.push(`**Branch:** ${output.commit.branch}`);
    }
  }

  // Analysis scope (only in structured output)
  if (isStructuredOutput && output.analysis) {
    lines.push(`**Scope:** ${output.analysis.scope}`);
    lines.push(`**Tool Version:** ${output.analysis.tool_version}\n`);
  }

  // Function count and risk breakdown
  if (functions && Array.isArray(functions)) {
    const total = functions.length;
    const byBand = {
      critical: functions.filter((f: any) => f.band === 'critical').length,
      high: functions.filter((f: any) => f.band === 'high').length,
      moderate: functions.filter((f: any) => f.band === 'moderate').length,
      low: functions.filter((f: any) => f.band === 'low').length,
    };

    lines.push(`**Total Functions:** ${total}`);
    lines.push(`**Risk Breakdown:**`);
    lines.push(`  - Critical: ${byBand.critical}`);
    lines.push(`  - High: ${byBand.high}`);
    lines.push(`  - Moderate: ${byBand.moderate}`);
    lines.push(`  - Low: ${byBand.low}\n`);

    // List critical and high-risk functions
    const highRisk = functions.filter(
      (f: any) => f.band === 'critical' || f.band === 'high'
    );

    if (highRisk.length > 0) {
      lines.push(`## High-Risk Functions (${highRisk.length})\n`);
      highRisk.slice(0, 10).forEach((func: any) => {
        const file = path.basename(func.file);
        const id = func.function_id?.split('::')[1] || func.function_id || func.function || 'anonymous';
        lines.push(`- **${id}** in ${file}:${func.line} - LRS: ${func.lrs.toFixed(2)} (${func.band})`);
      });

      if (highRisk.length > 10) {
        lines.push(`\n_...and ${highRisk.length - 10} more_`);
      }
    }
  }

  // Policy results (only in structured output)
  if (isStructuredOutput && output.policy_results) {
    const failed = output.policy_results.failed || [];
    const warnings = output.policy_results.warnings || [];

    if (failed.length > 0) {
      lines.push(`\n## ❌ Policy Failures (${failed.length})\n`);
      failed.slice(0, 5).forEach((violation: any) => {
        lines.push(`- ${violation.message}`);
      });
    }

    if (warnings.length > 0) {
      lines.push(`\n## ⚠️ Warnings (${warnings.length})\n`);
      warnings.slice(0, 5).forEach((warning: any) => {
        lines.push(`- ${warning.message}`);
      });
    }
  }

  return lines.join('\n');
}
