// Cross-runtime: structurally equal objects remain distinct Map keys.
const a = { x: 1 };
const b = { x: 1 };
const m = new Map<any, string>([[a, "a"], [b, "b"]]);
console.log(m.size, m.get(a), m.get(b), m.get({ x: 1 }));
a.x = 9;
console.log(m.get(a), [...m.keys()].map((k) => k.x).join(","));

