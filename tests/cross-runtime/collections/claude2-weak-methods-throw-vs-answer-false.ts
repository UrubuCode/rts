// Cross-runtime: the weak collections split their methods in two. `set`/`add`
// REFUSE a value that cannot be held weakly with a TypeError, while
// `get`/`has`/`delete` answer undefined/false for the very same value — a
// lookup with a primitive is a miss, not an error.

const wm = new WeakMap<any, string>();
const ws = new WeakSet<any>();
const held: any = { id: "held" };
wm.set(held, "v");
ws.add(held);

const primitives: any[] = [1, "s", true, null, undefined, 10n, NaN, -0];

// --- the writing side throws for every primitive ---
let writes = "";
for (const p of primitives) {
  try { wm.set(p, "x"); writes += String(typeof p) + ":ok "; }
  catch (e: any) { writes += String(typeof p) + ":" + e.constructor.name + " "; }
}
console.log("weakmap_set=" + writes.trim());

let adds = "";
for (const p of primitives) {
  try { ws.add(p); adds += String(typeof p) + ":ok "; }
  catch (e: any) { adds += String(typeof p) + ":" + e.constructor.name + " "; }
}
console.log("weakset_add=" + adds.trim());

// --- the reading side answers quietly ---
let reads = "";
for (const p of primitives) {
  try { reads += String(typeof p) + ":" + String(wm.get(p)) + " "; }
  catch (e: any) { reads += String(typeof p) + ":" + e.constructor.name + " "; }
}
console.log("weakmap_get=" + reads.trim());

let hasses = "";
for (const p of primitives) {
  try { hasses += String(typeof p) + ":" + wm.has(p) + " "; }
  catch (e: any) { hasses += String(typeof p) + ":" + e.constructor.name + " "; }
}
console.log("weakmap_has=" + hasses.trim());

let dels = "";
for (const p of primitives) {
  try { dels += String(typeof p) + ":" + wm.delete(p) + " "; }
  catch (e: any) { dels += String(typeof p) + ":" + e.constructor.name + " "; }
}
console.log("weakmap_delete=" + dels.trim());

let setHas = "";
for (const p of primitives) {
  try { setHas += String(typeof p) + ":" + ws.has(p) + " "; }
  catch (e: any) { setHas += String(typeof p) + ":" + e.constructor.name + " "; }
}
console.log("weakset_has=" + setHas.trim());

// --- the same split for a registered symbol, which cannot be held weakly ---
const reg = Symbol.for("claude2-weak-split");
function attempt(label: string, fn: () => any): void {
  try { console.log(label + "=ok:" + String(fn())); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}
attempt("registered_set", () => wm.set(reg as any, "x"));
attempt("registered_get", () => wm.get(reg as any));
attempt("registered_has", () => wm.has(reg as any));
attempt("registered_delete", () => wm.delete(reg as any));

// --- and no split at all for an unregistered symbol: it is a legal key ---
const uniq = Symbol("uniq");
attempt("unique_set", () => wm.set(uniq as any, "sym") === wm);
attempt("unique_get", () => wm.get(uniq as any));
attempt("unique_delete", () => wm.delete(uniq as any));

// --- the return values ---
console.log("set_returns_this=" + (wm.set(held, "v2") === wm));
console.log("add_returns_this=" + (ws.add(held) === ws));
console.log("get_hit=" + wm.get(held));
console.log("get_miss=" + String(wm.get({})));
console.log("delete_hit=" + wm.delete(held));
console.log("delete_again=" + wm.delete(held));
console.log("has_after_delete=" + wm.has(held));
wm.set(held, "v3");

// --- brand checks: the four surfaces do not accept each other ---
function brand(label: string, fn: () => any): void {
  try { console.log(label + "=ok:" + String(fn())); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}
brand("weakmap_get_on_map", () => WeakMap.prototype.get.call(new Map([[held, 1]]) as any, held));
brand("weakmap_get_on_weakset", () => WeakMap.prototype.get.call(ws as any, held));
brand("weakset_has_on_weakmap", () => WeakSet.prototype.has.call(wm as any, held));
brand("weakset_has_on_set", () => WeakSet.prototype.has.call(new Set([held]) as any, held));
brand("weakmap_get_on_plain", () => WeakMap.prototype.get.call({} as any, held));
brand("weakmap_get_on_prototype", () => (WeakMap.prototype as any).get(held));

// --- nothing enumerates a weak collection ---
brand("spread_weakset", () => [...(ws as any)].length);
console.log("weakmap_own_keys=" + Reflect.ownKeys(wm).length);
console.log("weakmap_json=" + JSON.stringify(wm));
console.log("weakset_tag=" + Object.prototype.toString.call(ws));

// --- the constructor takes the same iterables, and the adder is a lookup ---
console.log("from_pairs=" + new WeakMap([[held, "p"]]).get(held));
console.log("from_values=" + new WeakSet([held]).has(held));
console.log("null_arg=" + (typeof new WeakMap(null as any)) + ":" + (typeof new WeakSet(null as any)));

class LoudWeakSet extends WeakSet<any> {
  static hits = 0;
  add(v: any): this { LoudWeakSet.hits++; return super.add(v); }
}
const loud = new LoudWeakSet([held, { other: 1 }]);
console.log("subclass_adder_used=" + LoudWeakSet.hits + ":" + loud.has(held));

// --- a WeakMap key is object identity, never structure ---
const k1 = { same: 1 };
const k2 = { same: 1 };
const idm = new WeakMap<any, string>([[k1, "first"]]);
console.log("identity=" + idm.get(k1) + ":" + String(idm.get(k2)));
k1.same = 99;
console.log("identity_survives_mutation=" + idm.get(k1));
