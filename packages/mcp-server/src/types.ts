export type SampleTool = 'coordinate' | 'analyze' | 'policy_check';

export interface SampleRow {
  id: number;
  tool: SampleTool;
  taken_at: string;
  repo_path: string;
  commit_sha: string | null;
  input_json: string;
  output_json: string;
  exit_code: number | null;
  summary_json: string;
}

export interface CoordinateSummary {
  fileCount: number;
  concentratedCount: number;
}

export interface AnalyzeSummary {
  criticalCount: number;
  highCount: number;
  watchCount: number;
  okCount: number;
  maxLrs: number;
}

export interface PolicyCheckSummary {
  blockingViolations: number;
  warnings: number;
}

/** What every tool handler returns; maps 1:1 onto an MCP CallToolResult. */
export interface ToolResult {
  [key: string]: unknown;
  content: { type: 'text'; text: string }[];
  isError?: boolean;
}

export function ok(value: unknown): ToolResult {
  return { content: [{ type: 'text', text: JSON.stringify(value, null, 2) }] };
}

export function fail(message: string): ToolResult {
  return { content: [{ type: 'text', text: message }], isError: true };
}
