import type { FunctionReport } from '@hotspots/types';

/**
 * Prompt template for analyzing complexity
 */
export const ANALYZE_PROMPT = `
You are a code complexity analysis assistant. Review the provided Hotspots analysis output and identify areas of concern.

Focus on:
- Functions with "critical" or "high" risk bands
- Policy violations (failed or warnings)
- Functions with high individual metrics (CC, ND, FO, NS)
- Opportunities for refactoring

Provide specific, actionable suggestions for each high-risk function.
`.trim();

/**
 * Prompt template for refactoring suggestions
 */
export const REFACTOR_PROMPT = `
You are an expert code refactoring assistant. Your goal is to reduce code complexity while preserving functionality.

Given a function with the following complexity metrics:
- Current LRS: {lrs}
- Target LRS: {targetLrs}
- Cyclomatic Complexity (CC): {cc}
- Nesting Depth (ND): {nd}
- Fan-Out (FO): {fo}
- Non-Structured Exits (NS): {ns}

Provide a refactored version that:
1. Reduces complexity (aim for LRS < {targetLrs})
2. Maintains identical behavior
3. Preserves all test coverage
4. Uses clear, descriptive names

Focus on the metrics that contribute most to high LRS.
`.trim();

/**
 * Prompt template for explaining complexity
 */
export const EXPLAIN_PROMPT = `
You are a code complexity educator. Explain why this function has high complexity in plain English.

Function: {functionName}
LRS: {lrs} ({band})

Metrics:
- Cyclomatic Complexity (CC): {cc} - Number of decision points
- Nesting Depth (ND): {nd} - Maximum nesting level
- Fan-Out (FO): {fo} - Number of functions called
- Non-Structured Exits (NS): {ns} - Early returns, throws, etc.

Explain:
1. Which metrics contribute most to the high LRS
2. What patterns in the code cause these high values
3. Why this makes the function risky or hard to maintain
`.trim();

/**
 * Prompt template for complexity-constrained code generation
 */
export const GENERATE_WITH_CONSTRAINT_PROMPT = `
You are a code generation assistant. Generate clean, simple code that meets complexity constraints.

Target LRS: < {targetLrs} (moderate complexity or lower)

Guidelines:
- Prefer multiple small functions over one large function
- Avoid deep nesting (ND ≤ 2)
- Limit decision points (CC ≤ 10)
- Use early returns for validation
- Extract complex logic into helper functions

After generating code, verify it would pass complexity analysis.
`.trim();

/**
 * Create a refactoring prompt for a specific function
 */
export function createRefactorPrompt(
  func: FunctionReport,
  targetLrs: number = 6.0
): string {
  return REFACTOR_PROMPT.replace(/{lrs}/g, func.lrs.toFixed(1))
    .replace(/{targetLrs}/g, targetLrs.toFixed(1))
    .replace(/{cc}/g, func.metrics.cc.toString())
    .replace(/{nd}/g, func.metrics.nd.toString())
    .replace(/{fo}/g, func.metrics.fo.toString())
    .replace(/{ns}/g, func.metrics.ns.toString());
}

/**
 * Create an explanation prompt for a specific function
 */
export function createExplainPrompt(func: FunctionReport): string {
  const functionName = func.function_id.split('::')[1] || func.function_id;

  return EXPLAIN_PROMPT.replace(/{functionName}/g, functionName)
    .replace(/{lrs}/g, func.lrs.toFixed(1))
    .replace(/{band}/g, func.band)
    .replace(/{cc}/g, func.metrics.cc.toString())
    .replace(/{nd}/g, func.metrics.nd.toString())
    .replace(/{fo}/g, func.metrics.fo.toString())
    .replace(/{ns}/g, func.metrics.ns.toString());
}

/**
 * Create a generation prompt with complexity constraint
 */
export function createGeneratePrompt(
  description: string,
  targetLrs: number = 6.0
): string {
  return (
    GENERATE_WITH_CONSTRAINT_PROMPT.replace(/{targetLrs}/g, targetLrs.toFixed(1)) +
    '\n\n' +
    `Task: ${description}`
  );
}
