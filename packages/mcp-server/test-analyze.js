#!/usr/bin/env node

// Simple test of the analyze tool
const path = require('path');

// Set HOTSPOTS_PATH to the built binary
process.env.HOTSPOTS_PATH = path.resolve(__dirname, '../../target/release/hotspots');

// Import the analyze function
const { analyze } = require('./dist/tools/analyze.js');

async function test() {
  console.log('Testing hotspots_analyze tool...\n');

  try {
    // Test 1: Analyze the types package
    const testPath = path.resolve(__dirname, '../types/src/');
    console.log('Test 1: Analyzing', testPath);
    const result = await analyze({
      path: testPath,
      mode: 'snapshot'
    });

    if (result.success) {
      console.log('✓ Analysis succeeded\n');
      console.log(result.summary);
      console.log('\n✓ All tests passed!');
    } else {
      console.error('✗ Analysis failed:', result.error);
      process.exit(1);
    }
  } catch (error) {
    console.error('✗ Test failed:', error);
    process.exit(1);
  }
}

test();
