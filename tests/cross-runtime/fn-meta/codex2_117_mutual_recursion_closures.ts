// Cross-runtime: mutually recursive closures resolve later lexical bindings.
const even = (n: number): boolean => n === 0 || odd(n - 1);
const odd = (n: number): boolean => n !== 0 && even(n - 1);
console.log([0, 1, 7, 12].map((n) => even(n)).join(","));
console.log([0, 1, 7, 12].map((n) => odd(n)).join(","));

