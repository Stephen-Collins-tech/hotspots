// Fixture: known call relationships for call graph golden tests
// helper: fo=0, no calls
// middle: fo=1, calls helper (deduplication: helper() called twice = 1 unique callee)
// top:    fo=2, calls helper + middle

function helper(): number {
  return 1;
}

function middle(): number {
  return helper() + helper();
}

function top(): number {
  return helper() + middle();
}
