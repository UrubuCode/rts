// Cross-runtime: trampolining — safe recursion without stack overflow.

// Trampoline: keep calling returned functions until non-function value
function trampoline<T>(fn: (...args: any[]) => T | ((...args: any[]) => any)): (...args: any[]) => T {
  return function(...args: any[]): T {
    let result: any = fn(...args);
    while (typeof result === "function") result = result();
    return result;
  };
}

// Factorial via trampoline (avoids stack overflow for large n)
const factorial = trampoline(function fact(n: number, acc = 1): any {
  if (n <= 1) return acc;
  return () => fact(n - 1, n * acc);
});
console.log(factorial(10));    // 3628800
console.log(factorial(0));     // 1

// Fibonacci via trampoline
const fib = trampoline(function f(n: number, a = 0, b = 1): any {
  if (n === 0) return a;
  return () => f(n - 1, b, a + b);
});
console.log(fib(0));   // 0
console.log(fib(1));   // 1
console.log(fib(10));  // 55
console.log(fib(20));  // 6765

// Deep recursion that would stack overflow without trampoline
const sum = trampoline(function s(n: number, acc = 0): any {
  if (n <= 0) return acc;
  return () => s(n - 1, acc + n);
});
console.log(sum(1000));   // 500500
console.log(sum(10000));  // 50005000

// Continuation-passing style (CPS) for mutual recursion
function isEvenCPS(n: number, k: (v: boolean) => any): any {
  if (n === 0) return k(true);
  return () => isOddCPS(n - 1, k);
}
function isOddCPS(n: number, k: (v: boolean) => any): any {
  if (n === 0) return k(false);
  return () => isEvenCPS(n - 1, k);
}
function runCPS<T>(fn: (k: (v: T) => T) => any): T {
  let result: any;
  let thunk: any = fn((v: T) => { result = v; return v; });
  while (typeof thunk === "function") thunk = thunk();
  return result;
}

console.log(runCPS<boolean>(k => isEvenCPS(10, k)));  // true
console.log(runCPS<boolean>(k => isEvenCPS(11, k)));  // false
console.log(runCPS<boolean>(k => isOddCPS(7, k)));    // true
