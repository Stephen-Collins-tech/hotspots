// Nested branching
function nested(x, y) {
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
