// Cross-runtime compatibility: WeakMap/WeakSet object identity behavior.
const a = { id: 1 };
const b = { id: 2 };

const wm = new WeakMap();
wm.set(a, "alpha");
wm.set(b, "beta");

const ws = new WeakSet();
ws.add(a);

console.log("wm_a=" + wm.get(a));
console.log("wm_b=" + wm.get(b));
console.log("ws_a=" + ws.has(a));
console.log("ws_b=" + ws.has(b));
