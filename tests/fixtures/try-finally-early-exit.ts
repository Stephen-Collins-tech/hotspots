// try/finally (no catch) where try returns and finally always throws
// Validates that connect_finally does not produce an orphaned join node
// when the finally block always terminates
function alwaysThrowsInFinally(x: number): number {
  try {
    return x * 2;
  } finally {
    throw new Error("always");
  }
}
