#!/usr/bin/env tsx

/**
 * PR Reviewer Example
 *
 * Analyzes pull requests for complexity regressions and posts review comments.
 * Integrates with GitHub API and AI services.
 *
 * Usage:
 *   ANTHROPIC_API_KEY=sk-ant-... GITHUB_TOKEN=ghp_... tsx pr-reviewer/pr-reviewer.ts <pr-number>
 */

import { HotspotsClient } from '../shared/hotspots-client.js';
import { getViolations, getHighRiskFunctions, formatFunctionList } from '../shared/result-parser.js';
import Anthropic from '@anthropic-ai/sdk';

interface PRReviewConfig {
  owner: string;
  repo: string;
  prNumber: number;
  minLrs: number;
}

async function reviewPR(config: PRReviewConfig): Promise<void> {
  console.log(`\n🔍 Reviewing PR #${config.prNumber} in ${config.owner}/${config.repo}...\n`);

  const client = new HotspotsClient();
  const anthropic = new Anthropic({
    apiKey: process.env.ANTHROPIC_API_KEY,
  });

  // Step 1: Analyze PR changes
  console.log('📊 Running Hotspots analysis...');

  const analysis = await client.analyze({
    path: '.',
    mode: 'delta',
    policies: true,
    minLrs: config.minLrs,
  });

  const violations = getViolations(analysis);
  const highRisk = getHighRiskFunctions(analysis, config.minLrs);

  console.log(`   Found ${violations.length} violations`);
  console.log(`   Found ${highRisk.length} high-risk functions\n`);

  // Step 2: Generate review comment with AI
  if (violations.length === 0 && highRisk.length === 0) {
    console.log('✅ No complexity issues found in this PR');
    return;
  }

  console.log('🤖 Generating review comment with AI...');

  const reviewContext = `
Hotspots Complexity Analysis Results for PR #${config.prNumber}:

Violations: ${violations.length}
${violations.map(v => `- ${v.message}`).join('\n')}

High-Risk Functions: ${highRisk.length}
${formatFunctionList(highRisk)}

Generate a helpful, constructive PR review comment that:
1. Summarizes the complexity findings
2. Highlights the most critical issues
3. Suggests specific refactoring strategies
4. Is friendly and encouraging (not accusatory)
`;

  const message = await anthropic.messages.create({
    model: 'claude-3-5-sonnet-20241022',
    max_tokens: 1024,
    messages: [
      {
        role: 'user',
        content: reviewContext,
      },
    ],
  });

  const reviewComment = message.content[0].type === 'text' ? message.content[0].text : '';

  console.log('\n💬 Generated review comment:\n');
  console.log('---');
  console.log(reviewComment);
  console.log('---\n');

  // Step 3: Post comment to GitHub
  console.log('📝 Posting comment to PR...');
  console.log('   (In production: use Octokit to post comment via GitHub API)');

  // In a real implementation:
  // const octokit = new Octokit({ auth: process.env.GITHUB_TOKEN });
  // await octokit.rest.issues.createComment({
  //   owner: config.owner,
  //   repo: config.repo,
  //   issue_number: config.prNumber,
  //   body: reviewComment,
  // });

  console.log('\n✅ Review complete!');
}

// Main execution
if (import.meta.url === `file://${process.argv[1]}`) {
  const prNumber = parseInt(process.argv[2], 10);

  if (!prNumber || isNaN(prNumber)) {
    console.error('Usage: tsx pr-reviewer.ts <pr-number>');
    console.error('\nExample:\n  tsx pr-reviewer.ts 42');
    process.exit(1);
  }

  if (!process.env.ANTHROPIC_API_KEY) {
    console.error('Error: ANTHROPIC_API_KEY environment variable not set');
    process.exit(1);
  }

  const config: PRReviewConfig = {
    owner: 'yourorg',
    repo: 'yourrepo',
    prNumber,
    minLrs: 6.0,
  };

  reviewPR(config).catch((error) => {
    console.error('Error:', error);
    process.exit(1);
  });
}

export { reviewPR };
