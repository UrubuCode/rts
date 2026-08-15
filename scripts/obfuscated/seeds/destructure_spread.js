// Destructuring, defaults, rest/spread, computed keys, iterables.
const out = [];
const { a, b: bee = 9, ...rest } = { a: 1, c: 3, d: 4 };
out.push(a, bee, JSON.stringify(rest));
const [p, , q = 7, ...tail] = [1, 2, undefined, 4, 5];
out.push(p, q, tail.join(","));
function f({ x = 1, y = 2 } = {}, ...more) { return x + y + more.length; }
out.push(f(), f({ x: 10 }, 1, 2));
const key = "dyn";
const o = { [key + "1"]: "v1", [`${key}2`]: "v2" };
out.push(Object.keys(o).join(","));
const merged = { ...o, extra: true };
out.push(Object.keys(merged).length);
function* gen() { yield 1; yield 2; yield 3; }
out.push([...gen()].join("+"));
const [g1, ...grest] = gen();
out.push(g1, grest.join("."));
const m = new Map([["k", 1], ["j", 2]]);
out.push([...m.entries()].map(([k, v]) => k + v).join(","));
for (const [k, v] of m) out.push(k + "=" + v);
console.log(out.join("|"));
