#!/usr/bin/env tsx

/**
 * Refactor Loop Example
 *
 * Demonstrates an AI-driven refactoring loop that iteratively reduces complexity
 * until a target LRS is achieved.
 *
 * Usage:
 *   OPENAI_API_KEY=sk-... tsx refactor-loop/refactor-loop.ts <file-path> <function-name>
 */

import { HotspotsClient } from '../shared/hotspots-client.js';
import { createRefactorPrompt } from '../shared/ai-prompts.js';
import { getHighRiskFunctions, formatFunctionSummary } from '../shared/result-parser.js';
import OpenAI from 'openai';
import * as fs from 'fs/promises';

interface RefactorConfig {
  targetLrs: number;
  maxIterations: number;
  testCommand?: string;
}

const DEFAULT_CONFIG: RefactorConfig = {
  targetLrs: 6.0,
  maxIterations: 3,
  testCommand: 'npm test',
};

async function refactorLoop(
  filePath: string,
  functionName: string,
  config: RefactorConfig = DEFAULT_CONFIG
): Promise<void> {
  const client = new HotspotsClient();
  const openai = new OpenAI({
    apiKey: process.env.OPENAI_API_KEY,
  });

  console.log(`\n🔍 Analyzing ${filePath}...`);

  // Step 1: Initial analysis
  const initialAnalysis = await client.analyzeFile(filePath, 'snapshot');
  const targetFunc = initialAnalysis.functions.find((fn) =>
    fn.function_id.includes(functionName)
  );

  if (!targetFunc) {
    console.error(`❌ Function "${functionName}" not found in ${filePath}`);
    return;
  }

  console.log(`\n📊 Initial complexity: ${formatFunctionSummary(targetFunc)}`);

  if (targetFunc.lrs < config.targetLrs) {
    console.log(`✅ Function already meets target LRS (< ${config.targetLrs})`);
    return;
  }

  // Step 2: Refactoring loop
  let currentLrs = targetFunc.lrs;
  let iteration = 0;

  while (iteration < config.maxIterations && currentLrs >= config.targetLrs) {
    iteration++;
    console.log(`\n🔄 Iteration ${iteration}/${config.maxIterations}`);

    // Read current function code
    const fileContent = await fs.readFile(filePath, 'utf-8');

    // Generate refactor prompt
    const prompt = createRefactorPrompt(targetFunc, config.targetLrs);

    console.log(`🤖 Asking AI for refactoring suggestions...`);

    // Call AI for suggestions
    const response = await openai.chat.completions.create({
      model: 'gpt-4',
      messages: [
        {
          role: 'system',
          content:
            'You are an expert code refactoring assistant focused on reducing complexity.',
        },
        {
          role: 'user',
          content: `${prompt}\n\nCurrent code:\n\`\`\`typescript\n${fileContent}\n\`\`\``,
        },
      ],
      temperature: 0.3,
    });

    const suggestion = response.choices[0].message.content;

    if (!suggestion) {
      console.log(`⚠️  AI returned no suggestions`);
      break;
    }

    console.log(`\n💡 AI Suggestion:\n${suggestion}`);

    // In a real implementation, you would:
    // 1. Extract code from AI response
    // 2. Apply the refactoring
    // 3. Run tests
    // 4. Re-analyze
    // 5. Check if LRS improved

    console.log(
      `\n⚠️  Note: This is a simplified example. In production, you would:`
    );
    console.log(`  1. Parse and apply the AI's code changes`);
    console.log(`  2. Run your test suite to verify correctness`);
    console.log(`  3. Re-run Hotspots to measure improvement`);
    console.log(`  4. Revert changes if tests fail or complexity increased`);

    break; // Exit loop for this simplified example
  }

  if (currentLrs < config.targetLrs) {
    console.log(
      `\n✅ Success! Reduced LRS from ${targetFunc.lrs.toFixed(1)} to ${currentLrs.toFixed(1)}`
    );
  } else {
    console.log(
      `\n⚠️  Could not reach target LRS after ${iteration} iterations`
    );
    console.log(`   Current: ${currentLrs.toFixed(1)}, Target: < ${config.targetLrs}`);
  }
}

// Main execution
if (import.meta.url === `file://${process.argv[1]}`) {
  const filePath = process.argv[2];
  const functionName = process.argv[3];

  if (!filePath || !functionName) {
    console.error('Usage: tsx refactor-loop.ts <file-path> <function-name>');
    process.exit(1);
  }

  if (!process.env.OPENAI_API_KEY) {
    console.error('Error: OPENAI_API_KEY environment variable not set');
    process.exit(1);
  }

  refactorLoop(filePath, functionName).catch((error) => {
    console.error('Error:', error);
    process.exit(1);
  });
}

export { refactorLoop };
