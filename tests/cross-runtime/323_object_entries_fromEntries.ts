// Cross-runtime: Object.entries round-trip via Object.fromEntries.
const obj = { a: 1, b: 2, c: 3 };
const entries = Object.entries(obj);
console.log(entries.map(([k, v]) => k + "=" + v).join(","));

const rebuilt = Object.fromEntries(entries);
console.log(rebuilt.a);
console.log(rebuilt.b);
console.log(rebuilt.c);

// fromEntries from Map
const m = new Map([["x", 10], ["y", 20]]);
const fromMap = Object.fromEntries(m);
console.log(fromMap.x);
console.log(fromMap.y);

// empty
console.log(JSON.stringify(Object.fromEntries([])));
