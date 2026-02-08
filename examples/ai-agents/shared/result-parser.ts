import type { HotspotsOutput, FunctionReport, PolicyViolation } from '@hotspots/types';

/**
 * Parse Hotspots JSON output from string
 *
 * @param json - JSON string from hotspots output
 * @returns Parsed HotspotsOutput
 * @throws {SyntaxError} If JSON is invalid
 */
export function parseHotspotsOutput(json: string): HotspotsOutput {
  return JSON.parse(json) as HotspotsOutput;
}

/**
 * Get functions above a minimum LRS threshold
 *
 * @param output - Hotspots analysis output
 * @param minLrs - Minimum LRS threshold (default: 6.0)
 * @returns Array of high-risk functions, sorted by LRS descending
 */
export function getHighRiskFunctions(
  output: HotspotsOutput,
  minLrs: number = 6.0
): FunctionReport[] {
  return output.functions
    .filter((fn) => fn.lrs >= minLrs)
    .sort((a, b) => b.lrs - a.lrs);
}

/**
 * Get policy violations from analysis output
 *
 * @param output - Hotspots analysis output
 * @param severity - Optional filter by severity ('blocking' or 'warning')
 * @returns Array of violations
 */
export function getViolations(
  output: HotspotsOutput,
  severity?: 'blocking' | 'warning'
): PolicyViolation[] {
  if (!output.policy_results) {
    return [];
  }

  const violations: PolicyViolation[] = [];

  if (!severity || severity === 'blocking') {
    violations.push(...(output.policy_results.failed || []));
  }

  if (!severity || severity === 'warning') {
    violations.push(...(output.policy_results.warnings || []));
  }

  return violations;
}

/**
 * Get only changed functions from delta analysis
 *
 * @param output - Hotspots delta analysis output
 * @returns Array of new or modified functions
 */
export function getChangedFunctions(output: HotspotsOutput): FunctionReport[] {
  if (output.analysis.scope !== 'delta') {
    throw new Error('getChangedFunctions() requires delta mode output');
  }

  return output.functions.filter(
    (fn) => fn.delta_type === 'added' || fn.delta_type === 'modified'
  );
}

/**
 * Format a function report as a human-readable summary
 *
 * @param fn - Function report
 * @returns Formatted string
 *
 * @example
 * ```
 * "handleRequest (src/api.ts:88) - LRS 11.2 (critical)"
 * ```
 */
export function formatFunctionSummary(fn: FunctionReport): string {
  const functionName = fn.function_id.split('::')[1] || fn.function_id;
  const file = fn.file.replace(process.cwd(), '');
  return `${functionName} (${file}:${fn.line}) - LRS ${fn.lrs.toFixed(1)} (${fn.band})`;
}

/**
 * Format multiple functions as a numbered list
 *
 * @param functions - Array of function reports
 * @returns Formatted multi-line string
 */
export function formatFunctionList(functions: FunctionReport[]): string {
  return functions.map((fn, i) => `${i + 1}. ${formatFunctionSummary(fn)}`).join('\n');
}

/**
 * Get risk band summary statistics
 *
 * @param output - Hotspots analysis output
 * @returns Object with counts per risk band
 */
export function getRiskBandSummary(output: HotspotsOutput): {
  critical: number;
  high: number;
  moderate: number;
  low: number;
  total: number;
} {
  const summary = {
    critical: 0,
    high: 0,
    moderate: 0,
    low: 0,
    total: output.functions.length,
  };

  for (const fn of output.functions) {
    summary[fn.band]++;
  }

  return summary;
}
