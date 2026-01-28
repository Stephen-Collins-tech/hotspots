// Pathological complexity
function pathological(a, b, c, d) {
  let result = 0;

  // Multiple nested conditions
  if (a > 0 && b > 0 || c > 0) {
    if (a > b) {
      if (b > c) {
        for (let i = 0; i < a; i++) {
          if (i % 2 === 0) {
            switch (i % 3) {
              case 0:
                result += i;
                break;
              case 1:
                result += i * 2;
                break;
              default:
                break;
            }
          }
        }
      }
    }
  }

  try {
    if (d > 0) {
      result += d;
    }
  } catch (e) {
    result -= 1;
  }

  return result;
}
