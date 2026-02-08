#!/usr/bin/env tsx

/**
 * Pre-Commit Review Example
 *
 * Analyzes staged changes for complexity violations before commit.
 * Can be used as a git pre-commit hook or run manually.
 *
 * Usage:
 *   tsx pre-commit-review/pre-commit-review.ts
 *
 * Exit codes:
 *   0 - No violations, safe to commit
 *   1 - Violations found, commit blocked
 */

import { HotspotsClient } from '../shared/hotspots-client.js';
import {
  getViolations,
  getHighRiskFunctions,
  formatFunctionList,
} from '../shared/result-parser.js';

async function preCommitReview(): Promise<number> {
  console.log('🔍 Running Hotspots pre-commit review...\n');

  const client = new HotspotsClient();

  try {
    // Analyze staged changes in delta mode with policies
    const analysis = await client.analyze({
      path: '.',
      mode: 'delta',
      policies: true,
    });

    // Check for blocking violations
    const blockingViolations = getViolations(analysis, 'blocking');
    const warnings = getViolations(analysis, 'warning');
    const highRisk = getHighRiskFunctions(analysis, 9.0); // Critical only

    // Report results
    if (blockingViolations.length === 0 && highRisk.length === 0) {
      console.log('✅ No complexity violations detected\n');

      if (warnings.length > 0) {
        console.log(`⚠️  ${warnings.length} warning(s):\n`);
        warnings.forEach((w) => {
          console.log(`  - ${w.message}`);
        });
        console.log();
      }

      console.log('✓ Safe to commit');
      return 0;
    }

    // Report violations
    if (blockingViolations.length > 0) {
      console.log(`❌ ${blockingViolations.length} blocking violation(s):\n`);
      blockingViolations.forEach((v) => {
        console.log(`  - ${v.message}`);
      });
      console.log();
    }

    if (highRisk.length > 0) {
      console.log(`❌ ${highRisk.length} critical complexity function(s):\n`);
      console.log(formatFunctionList(highRisk));
      console.log();
    }

    console.log('⚠️  Commit blocked due to complexity violations');
    console.log('\nOptions:');
    console.log('  1. Refactor high-complexity code');
    console.log('  2. Add suppression comment: // hotspots-ignore: reason');
    console.log('  3. Skip check: git commit --no-verify');

    return 1;
  } catch (error: any) {
    console.error(`Error running analysis: ${error.message}`);
    return 1;
  }
}

// Main execution
if (import.meta.url === `file://${process.argv[1]}`) {
  preCommitReview()
    .then((exitCode) => {
      process.exit(exitCode);
    })
    .catch((error) => {
      console.error('Fatal error:', error);
      process.exit(1);
    });
}

export { preCommitReview };
