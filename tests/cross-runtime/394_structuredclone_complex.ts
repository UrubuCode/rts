// Cross-runtime: structuredClone with Map, Set, Date, RegExp, nested.

// Map
const m = new Map([["a", 1], ["b", new Map([["c", 3]])]]);
const mc = structuredClone(m);
(mc.get("b") as Map<string, number>).set("c", 99);
console.log((m.get("b") as Map<string, number>).get("c")); // 3 — original unchanged
console.log((mc.get("b") as Map<string, number>).get("c")); // 99

// Set
const s = new Set([1, 2, 3, new Set([4, 5])]);
const sc = structuredClone(s);
// Inner set — add to clone doesn't affect original
const [innerOrig] = [...s].filter(v => v instanceof Set) as Set<number>[];
const [innerClone] = [...sc].filter(v => v instanceof Set) as Set<number>[];
innerClone.add(99);
console.log(innerOrig.has(99));   // false
console.log(innerClone.has(99));  // true

// Date
const d = new Date("2026-01-01T00:00:00Z");
const dc = structuredClone(d);
dc.setUTCFullYear(2099);
console.log(d.getUTCFullYear());   // 2026
console.log(dc.getUTCFullYear());  // 2099
console.log(dc instanceof Date); // true

// RegExp
const re = /hello/gi;
const rec = structuredClone(re);
console.log(rec.source);  // hello
console.log(rec.flags);   // gi
console.log(rec === re);  // false — different object

// Nested complex
const complex = {
  map: new Map([["key", new Set([1, 2, 3])]]),
  date: new Date("2026-05-22T00:00:00Z"),
  arr: [1, { nested: true }],
  re: /test/i,
};
const cloned = structuredClone(complex);
(cloned.map.get("key") as Set<number>).add(99);
cloned.arr[1] = { nested: false };

console.log((complex.map.get("key") as Set<number>).has(99));  // false
console.log((cloned.map.get("key") as Set<number>).has(99));   // true
console.log((complex.arr[1] as any).nested);  // true
console.log((cloned.arr[1] as any).nested);   // false
console.log(cloned.date instanceof Date);     // true
console.log(cloned.re.source);                // test

// Circular reference
const circ: any = { a: 1 };
circ.self = circ;
const cc = structuredClone(circ);
console.log(cc.a);           // 1
console.log(cc.self === cc); // true — circular preserved
console.log(cc === circ);    // false — deep clone
