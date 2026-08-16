// Cross-runtime: WeakSet tracks object identity and supports deletion.
const a = {};
const b = {};
const w = new WeakSet<object>([a]);
w.add(b).add(a);
console.log(w.has(a), w.has(b));
console.log(w.delete(a), w.has(a), w.delete(a));

