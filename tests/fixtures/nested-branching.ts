// Nested branching
function nested(x: number, y: number): number {
  if (x > 0) {
    if (y > 0) {
      return x + y;
    } else {
      return x - y;
    }
  } else {
    if (y > 0) {
      return y - x;
    } else {
      return -(x + y);
    }
  }
}
