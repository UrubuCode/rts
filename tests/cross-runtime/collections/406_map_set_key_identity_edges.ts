// Cross-runtime: Map/Set key identity with NaN, -0, objects, and arrays.
const m = new Map<any, string>();
const o1 = { x: 1 };
const o2 = { x: 1 };
m.set(NaN, "nan1");
m.set(Number("x"), "nan2");
m.set(-0, "negzero");
m.set(0, "zero");
m.set(o1, "o1");
m.set(o2, "o2");

console.log(m.size);
console.log(m.get(NaN));
console.log(m.get(-0) + ":" + m.get(0));
console.log(m.get(o1) + ":" + m.get(o2));

const s = new Set<any>([NaN, NaN, -0, 0, [1], [1]]);
console.log(s.size + ":" + s.has(NaN) + ":" + s.has(-0));
