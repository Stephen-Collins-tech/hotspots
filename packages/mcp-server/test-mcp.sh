#!/bin/bash

# Test the MCP server by sending a tool call request
# This simulates what Claude would send

# Add hotspots to PATH for testing
export PATH="$PWD/../../target/release:$PATH"

# Verify hotspots is available
which hotspots
if [ $? -ne 0 ]; then
  echo "Error: hotspots not found in PATH"
  exit 1
fi

echo "Testing MCP server..."
echo ""

# Start the MCP server and send a test request
# MCP uses JSON-RPC over stdio
echo '{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/list"
}' | node dist/index.js

echo ""
echo "Test complete"
