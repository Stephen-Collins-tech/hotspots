#!/usr/bin/env node

import { startServer } from './server.js';

// Start the MCP server
startServer().catch((error) => {
  console.error('Fatal error starting server:', error);
  process.exit(1);
});
