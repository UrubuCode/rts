// Cross-runtime: relational operators preserve their specified coercion ordering.
const seen: string[] = [];
const make = (name: string, n: number) => ({
  [Symbol.toPrimitive]() { seen.push(name); return n; },
});
const a = make("a", 1);
const b = make("b", 2);
console.log(a < b, a > b, a <= b, a >= b);
console.log(seen.join(","));

