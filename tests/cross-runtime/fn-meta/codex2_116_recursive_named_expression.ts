// Cross-runtime: a named function expression can recurse without leaking its name.
const factorial = function fact(n: number): number {
  return n < 2 ? 1 : n * fact(n - 1);
};
console.log(factorial(6));
console.log(typeof (globalThis as any).fact);

