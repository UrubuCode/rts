// Cross-runtime: WeakMap/WeakSet reject non-object keys.
const wm = new WeakMap<object, number>();
const ws = new WeakSet<object>();
const obj = {};
wm.set(obj, 7);
ws.add(obj);
console.log(wm.get(obj) + ":" + ws.has(obj));

for (const op of ["wm", "ws"]) {
  try {
    if (op === "wm") (wm as any).set(1, 2);
    else (ws as any).add("x");
  } catch (e: any) {
    console.log(op + ":" + e.constructor.name);
  }
}
