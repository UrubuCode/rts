// Cross-runtime: WeakMap stores object identities and updates existing keys.
const a = {};
const b = {};
const w = new WeakMap<object, number>();
w.set(a, 1).set(b, 2).set(a, 3);
console.log(w.get(a), w.get(b), w.has(a));
console.log(w.delete(a), w.has(a), w.get(a));

