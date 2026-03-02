// if/else where both branches return — validates no spurious edges and CC=1
function bothBranchesReturn(x: number): number {
  if (x > 0) {
    return x;
  } else {
    return -x;
  }
}
