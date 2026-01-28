// Loop with breaks
function loopWithBreaks(arr) {
  let sum = 0;
  for (const item of arr) {
    if (item < 0) {
      break;
    }
    if (item > 100) {
      continue;
    }
    sum += item;
  }
  return sum;
}
