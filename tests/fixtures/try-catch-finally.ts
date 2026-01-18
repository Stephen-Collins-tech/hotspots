// Try/catch/finally
function tryCatchFinally(x: number): number {
  let result = 0;
  try {
    result = x * 2;
    if (result > 100) {
      throw new Error("Too large");
    }
  } catch (e) {
    result = -1;
  } finally {
    result += 1;
  }
  return result;
}
