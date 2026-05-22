// Cross-runtime: closure patterns that bundlers optimize.

// Memoization (closure over cache)
function memoize<T>(fn: (...args: number[]) => T) {
  const cache = new Map<string, T>();
  return (...args: number[]) => {
    const key = args.join(",");
    if (cache.has(key)) return cache.get(key)!;
    const result = fn(...args);
    cache.set(key, result);
    return result;
  };
}

let callCount = 0;
const fib = memoize(function f(n: number): number {
  callCount++;
  if (n <= 1) return n;
  return f(n - 1) + f(n - 2);
});
console.log(fib(10));
console.log(fib(10));  // cached — callCount not incremented again

// Partial application
function partial<T>(fn: (...args: any[]) => T, ...preArgs: any[]) {
  return (...args: any[]) => fn(...preArgs, ...args);
}
const add = (a: number, b: number, c: number) => a + b + c;
const add10 = partial(add, 10);
console.log(add10(1, 2));
console.log(add10(5, 5));

// Once (run only once — common in bundlers for init)
function once<T>(fn: () => T) {
  let called = false;
  let result: T;
  return () => {
    if (!called) { called = true; result = fn(); }
    return result;
  };
}
let initCount = 0;
const init = once(() => { initCount++; return "initialized"; });
console.log(init());
console.log(init());
console.log(init());
console.log(initCount);  // 1

// Curry
function curry(fn: (...args: number[]) => number) {
  const arity = fn.length;
  return function curried(...args: number[]): any {
    if (args.length >= arity) return fn(...args);
    return (...more: number[]) => curried(...args, ...more);
  };
}
const multiply = curry((a: number, b: number, c: number) => a * b * c);
console.log(multiply(2)(3)(4));
console.log(multiply(2, 3)(4));
console.log(multiply(2, 3, 4));
