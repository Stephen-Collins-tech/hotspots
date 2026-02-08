#!/usr/bin/env tsx

/**
 * Constrained Code Generation Example
 *
 * Demonstrates generating code with complexity constraints.
 * AI regenerates code if it exceeds target LRS.
 *
 * Usage:
 *   OPENAI_API_KEY=sk-... tsx constrained-generation/constrained-generation.ts "<task description>"
 */

import { HotspotsClient } from '../shared/hotspots-client.js';
import { createGeneratePrompt } from '../shared/ai-prompts.js';
import { getHighRiskFunctions } from '../shared/result-parser.js';
import OpenAI from 'openai';
import * as fs from 'fs/promises';
import * as path from 'path';

interface GenerateConfig {
  targetLrs: number;
  maxAttempts: number;
  outputPath?: string;
}

const DEFAULT_CONFIG: GenerateConfig = {
  targetLrs: 6.0,
  maxAttempts: 3,
  outputPath: './generated.ts',
};

async function generateWithConstraint(
  description: string,
  config: GenerateConfig = DEFAULT_CONFIG
): Promise<string | null> {
  const client = new HotspotsClient();
  const openai = new OpenAI({
    apiKey: process.env.OPENAI_API_KEY,
  });

  const tempFile = config.outputPath || './temp-generated.ts';

  console.log(`\n🤖 Generating code: "${description}"`);
  console.log(`📏 Target complexity: LRS < ${config.targetLrs}\n`);

  for (let attempt = 1; attempt <= config.maxAttempts; attempt++) {
    console.log(`\n🔄 Attempt ${attempt}/${config.maxAttempts}`);

    // Generate code with complexity constraint in prompt
    const prompt = createGeneratePrompt(description, config.targetLrs);

    const response = await openai.chat.completions.create({
      model: 'gpt-4',
      messages: [
        {
          role: 'system',
          content:
            'You are a code generation assistant. Generate clean, simple TypeScript code that meets complexity constraints.',
        },
        {
          role: 'user',
          content: prompt,
        },
      ],
      temperature: 0.5,
    });

    const generatedCode = response.choices[0].message.content;

    if (!generatedCode) {
      console.log(`⚠️  AI returned no code`);
      continue;
    }

    // Extract code from markdown if needed
    let code = generatedCode;
    const codeBlockMatch = generatedCode.match(/```(?:typescript|ts)?\n([\s\S]+?)\n```/);
    if (codeBlockMatch) {
      code = codeBlockMatch[1];
    }

    // Write to temp file
    await fs.writeFile(tempFile, code);

    // Analyze complexity
    console.log(`📊 Analyzing complexity...`);

    try {
      const analysis = await client.analyzeFile(tempFile, 'snapshot');

      const maxLrs = Math.max(...analysis.functions.map((fn) => fn.lrs));
      console.log(`   Maximum LRS: ${maxLrs.toFixed(1)}`);

      if (maxLrs < config.targetLrs) {
        console.log(
          `\n✅ Success! Generated code meets complexity constraint (LRS < ${config.targetLrs})`
        );
        console.log(`\n📝 Generated code saved to: ${tempFile}`);
        return code;
      }

      const highRisk = getHighRiskFunctions(analysis, config.targetLrs);
      console.log(
        `⚠️  Code exceeds target (${highRisk.length} function(s) above threshold)`
      );

      if (attempt < config.maxAttempts) {
        console.log(`   Regenerating with stricter constraints...`);
      }
    } catch (error: any) {
      console.log(`⚠️  Analysis error: ${error.message}`);
    }
  }

  console.log(`\n❌ Could not generate code within complexity constraint after ${config.maxAttempts} attempts`);
  console.log(
    `\nSuggestions:\n  - Simplify the task description\n  - Increase targetLrs\n  - Break into smaller functions manually`
  );

  return null;
}

// Main execution
if (import.meta.url === `file://${process.argv[1]}`) {
  const description = process.argv[2];

  if (!description) {
    console.error(
      'Usage: tsx constrained-generation.ts "<task description>"'
    );
    console.error(
      '\nExample:\n  tsx constrained-generation.ts "function to validate user registration form"'
    );
    process.exit(1);
  }

  if (!process.env.OPENAI_API_KEY) {
    console.error('Error: OPENAI_API_KEY environment variable not set');
    process.exit(1);
  }

  generateWithConstraint(description).catch((error) => {
    console.error('Error:', error);
    process.exit(1);
  });
}

export { generateWithConstraint };
