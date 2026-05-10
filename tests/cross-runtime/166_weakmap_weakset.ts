// Cross-runtime: WeakMap and WeakSet basics
const wm = new WeakMap();
const key1 = {};
const key2 = {};

wm.set(key1, "value1");
wm.set(key2, "value2");

console.log("get1=" + wm.get(key1));
console.log("get2=" + wm.get(key2));
console.log("has1=" + wm.has(key1));

wm.delete(key1);
console.log("after_delete=" + wm.has(key1));

const ws = new WeakSet();
const obj1 = {};
const obj2 = {};

ws.add(obj1);
ws.add(obj2);

console.log("ws_has1=" + ws.has(obj1));
console.log("ws_has2=" + ws.has(obj2));

ws.delete(obj1);
console.log("ws_after=" + ws.has(obj1));
