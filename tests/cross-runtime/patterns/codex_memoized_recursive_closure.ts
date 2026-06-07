// Cross-runtime: memoized recursive closure with Map keys.
function memoFib() {
  const cache = new Map<number, number>([[0, 0], [1, 1]]);
  function fib(n: number): number {
    if (cache.has(n)) return cache.get(n)!;
    const v = fib(n - 1) + fib(n - 2);
    cache.set(n, v);
    return v;
  }
  return { fib, cache };
}

const m = memoFib();
console.log(m.fib(10));
console.log([...m.cache.keys()].join(","));
