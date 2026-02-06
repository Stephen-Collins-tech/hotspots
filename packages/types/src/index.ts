/**
 * @hotspots/types
 *
 * TypeScript types for Faultline complexity analysis output.
 * These types match the JSON Schema definitions in ../../schemas/
 *
 * @packageDocumentation
 */

/**
 * Risk band classification for function complexity
 */
export type RiskBand = "low" | "moderate" | "high" | "critical";

/**
 * Policy severity level
 */
export type Severity = "error" | "warning" | "info";

/**
 * Analysis mode
 */
export type AnalysisScope = "full" | "delta";

/**
 * Policy identifiers
 */
export type PolicyId =
  | "critical_threshold"
  | "high_threshold"
  | "critical_introduction"
  | "high_introduction"
  | "rapid_growth"
  | "net_repo_regression"
  | "watch_threshold"
  | "attention_threshold"
  | "suppression_missing_reason";

/**
 * Raw complexity metrics for a function
 *
 * @example
 * ```typescript
 * const metrics: Metrics = {
 *   cc: 8,   // Cyclomatic Complexity
 *   nd: 2,   // Nesting Depth
 *   fo: 4,   // Fan-Out
 *   ns: 2    // Non-Structured exits
 * };
 * ```
 */
export interface Metrics {
  /**
   * Cyclomatic Complexity - number of linearly independent paths through the code
   * (decision points + 1)
   */
  cc: number;

  /**
   * Nesting Depth - maximum level of nested control structures (if/while/for/try)
   */
  nd: number;

  /**
   * Fan-Out - number of distinct functions or methods called by this function
   */
  fo: number;

  /**
   * Non-Structured exits - number of early returns, throws, breaks, and continues
   */
  ns: number;
}

/**
 * Git commit information
 */
export interface CommitInfo {
  /** Git commit SHA (40-character hex string) */
  sha: string;

  /** Parent commit SHAs */
  parents: string[];

  /** Unix timestamp of commit */
  timestamp: number;

  /** Current branch name (if available) */
  branch?: string;
}

/**
 * Analysis metadata
 */
export interface AnalysisInfo {
  /** Analysis scope (full or delta) */
  scope: AnalysisScope;

  /** Version of Faultline that produced this output */
  tool_version: string;
}

/**
 * Complexity analysis result for a single function
 *
 * @example
 * ```typescript
 * const func: FunctionReport = {
 *   function_id: "/src/api.ts::handleRequest",
 *   file: "/Users/dev/project/src/api.ts",
 *   line: 42,
 *   metrics: { cc: 8, nd: 2, fo: 4, ns: 2 },
 *   lrs: 7.2,
 *   band: "high"
 * };
 * ```
 */
export interface FunctionReport {
  /**
   * Unique identifier for this function in the format 'file::functionName'
   */
  function_id: string;

  /**
   * Absolute path to the source file containing this function
   */
  file: string;

  /**
   * Line number where the function is defined
   */
  line: number;

  /**
   * Raw complexity metrics (CC, ND, FO, NS)
   */
  metrics: Metrics;

  /**
   * Logarithmic Risk Score - composite complexity metric combining all raw metrics
   * with logarithmic scaling. Higher is more complex.
   */
  lrs: number;

  /**
   * Risk band classification based on LRS thresholds
   */
  band: RiskBand;

  /**
   * Reason provided via // hotspots-ignore comment, if this function is suppressed
   * from policy checks
   */
  suppression_reason?: string;
}

/**
 * Aggregate statistics for a file
 */
export interface FileAggregate {
  /** File path */
  file: string;

  /** Sum of all LRS values in this file */
  sum_lrs: number;

  /** Maximum LRS value in this file */
  max_lrs: number;

  /** Number of high or critical functions in this file */
  high_plus_count: number;
}

/**
 * Aggregate statistics for a directory
 */
export interface DirectoryAggregate {
  /** Directory path */
  directory: string;

  /** Sum of all LRS values in this directory */
  sum_lrs: number;

  /** Maximum LRS value in this directory */
  max_lrs: number;

  /** Number of high or critical functions in this directory */
  high_plus_count: number;
}

/**
 * Aggregate statistics by file and directory
 */
export interface Aggregates {
  /** File-level aggregates */
  files: FileAggregate[];

  /** Directory-level aggregates */
  directories: DirectoryAggregate[];
}

/**
 * A single policy check result
 *
 * @example
 * ```typescript
 * const violation: PolicyResult = {
 *   id: "critical_threshold",
 *   severity: "error",
 *   function_id: "/src/complex.ts::processData",
 *   message: "Function exceeds critical threshold (LRS: 11.2 >= 9.0)",
 *   metadata: {
 *     file: "/src/complex.ts",
 *     line: 42,
 *     lrs: 11.2,
 *     band: "critical"
 *   }
 * };
 * ```
 */
export interface PolicyResult {
  /**
   * Policy identifier indicating which rule was violated
   */
  id: PolicyId;

  /**
   * Severity level of this result
   */
  severity: Severity;

  /**
   * Identifier of the function that violated the policy (if function-specific)
   */
  function_id?: string;

  /**
   * Human-readable message explaining the violation
   */
  message: string;

  /**
   * Additional structured information about the violation
   */
  metadata?: {
    file?: string;
    line?: number;
    lrs?: number;
    band?: RiskBand;
    previous_lrs?: number;
    delta_lrs?: number;
  };
}

/**
 * Policy evaluation results
 */
export interface PolicyResults {
  /** Blocking policy failures */
  failed: PolicyResult[];

  /** Non-blocking policy warnings */
  warnings: PolicyResult[];
}

/**
 * Complete Faultline analysis output (snapshot or delta mode)
 *
 * @example
 * ```typescript
 * const output: HotspotsOutput = {
 *   schema_version: 1,
 *   commit: {
 *     sha: "abc123...",
 *     parents: ["def456..."],
 *     timestamp: 1234567890,
 *     branch: "main"
 *   },
 *   analysis: {
 *     scope: "full",
 *     tool_version: "1.0.0"
 *   },
 *   functions: [
 *     {
 *       function_id: "/src/api.ts::handleRequest",
 *       file: "/src/api.ts",
 *       line: 42,
 *       metrics: { cc: 8, nd: 2, fo: 4, ns: 2 },
 *       lrs: 7.2,
 *       band: "high"
 *     }
 *   ]
 * };
 * ```
 */
export interface HotspotsOutput {
  /**
   * Schema version number for compatibility tracking
   */
  schema_version: number;

  /**
   * Git commit information for this analysis
   */
  commit: CommitInfo;

  /**
   * Metadata about the analysis run
   */
  analysis: AnalysisInfo;

  /**
   * Array of analyzed functions with their complexity metrics
   */
  functions: FunctionReport[];

  /**
   * Optional aggregate statistics by file and directory
   */
  aggregates?: Aggregates;

  /**
   * Policy evaluation results (only present when --policy is used)
   */
  policy_results?: PolicyResults;
}

//
// Type Guards
//

/**
 * Type guard to check if an object is a valid HotspotsOutput
 */
export function isHotspotsOutput(obj: unknown): obj is HotspotsOutput {
  if (typeof obj !== "object" || obj === null) return false;
  const o = obj as any;
  return (
    typeof o.schema_version === "number" &&
    typeof o.commit === "object" &&
    typeof o.analysis === "object" &&
    Array.isArray(o.functions)
  );
}

/**
 * Type guard to check if an object is a valid FunctionReport
 */
export function isFunctionReport(obj: unknown): obj is FunctionReport {
  if (typeof obj !== "object" || obj === null) return false;
  const o = obj as any;
  return (
    typeof o.function_id === "string" &&
    typeof o.file === "string" &&
    typeof o.line === "number" &&
    typeof o.metrics === "object" &&
    typeof o.lrs === "number" &&
    typeof o.band === "string"
  );
}

/**
 * Type guard to check if an object is a valid PolicyResult
 */
export function isPolicyResult(obj: unknown): obj is PolicyResult {
  if (typeof obj !== "object" || obj === null) return false;
  const o = obj as any;
  return (
    typeof o.id === "string" &&
    typeof o.severity === "string" &&
    typeof o.message === "string"
  );
}

//
// Helper Functions
//

/**
 * Filter functions by risk band
 *
 * @example
 * ```typescript
 * const highRisk = filterByRiskBand(output.functions, "high");
 * ```
 */
export function filterByRiskBand(
  functions: FunctionReport[],
  band: RiskBand
): FunctionReport[] {
  return functions.filter((f) => f.band === band);
}

/**
 * Filter policy results by severity
 *
 * @example
 * ```typescript
 * const errors = filterBySeverity(violations, "error");
 * ```
 */
export function filterBySeverity(
  results: PolicyResult[],
  severity: Severity
): PolicyResult[] {
  return results.filter((r) => r.severity === severity);
}

/**
 * Get the N functions with highest LRS
 *
 * @example
 * ```typescript
 * const top10 = getHighestRiskFunctions(output.functions, 10);
 * ```
 */
export function getHighestRiskFunctions(
  functions: FunctionReport[],
  n: number
): FunctionReport[] {
  return [...functions].sort((a, b) => b.lrs - a.lrs).slice(0, n);
}

/**
 * Get functions that exceed a specific LRS threshold
 *
 * @example
 * ```typescript
 * const overThreshold = getFunctionsAboveThreshold(output.functions, 6.0);
 * ```
 */
export function getFunctionsAboveThreshold(
  functions: FunctionReport[],
  threshold: number
): FunctionReport[] {
  return functions.filter((f) => f.lrs >= threshold);
}

/**
 * Check if policy check passed (no blocking failures)
 *
 * @example
 * ```typescript
 * if (output.policy_results && !policyPassed(output.policy_results)) {
 *   console.error("Policy check failed!");
 * }
 * ```
 */
export function policyPassed(results: PolicyResults): boolean {
  return results.failed.length === 0;
}
