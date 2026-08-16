// Cross-runtime: a finally return overrides an earlier try return.
function f(mode: number) {
  try {
    if (mode === 1) return "try";
    throw new Error("boom");
  } catch {
    return "catch";
  } finally {
    if (mode === 2) return "finally";
  }
}
console.log(f(1), f(0), f(2));

