// Cross-runtime: which values CAN be held weakly. An unregistered symbol is a
// legal WeakMap/WeakSet key (ES2023); a REGISTERED symbol from Symbol.for is
// not, and neither is any other primitive.

const wm = new WeakMap<any, any>();
const ws = new WeakSet<any>();

function tryKey(label: string, key: any): void {
  try {
    wm.set(key, "v");
    console.log("wm_" + label + "=ok:" + wm.get(key) + ":" + wm.has(key));
  } catch (e: any) {
    console.log("wm_" + label + "=" + e.constructor.name);
  }
  try {
    ws.add(key);
    console.log("ws_" + label + "=ok:" + ws.has(key));
  } catch (e: any) {
    console.log("ws_" + label + "=" + e.constructor.name);
  }
}

// --- an ordinary unique symbol is a valid weak key ---
const uniq = Symbol("uniq");
tryKey("unique_symbol", uniq);
console.log("uniq_delete=" + wm.delete(uniq) + ":" + wm.has(uniq));
console.log("uniq_delete_twice=" + wm.delete(uniq));

// --- a symbol with no description works too ---
tryKey("bare_symbol", Symbol());

// --- a well-known symbol is not in the registry, so it is holdable ---
tryKey("wellknown_symbol", Symbol.iterator);

// --- a registered symbol is refused ---
tryKey("registered_symbol", Symbol.for("claude-weak-key"));
console.log("registered_is_in_registry=" + (Symbol.keyFor(Symbol.for("claude-weak-key")) === "claude-weak-key"));
console.log("unique_not_in_registry=" + (Symbol.keyFor(uniq) === undefined));
console.log("wellknown_not_in_registry=" + (Symbol.keyFor(Symbol.iterator) === undefined));

// --- objects and functions have always been holdable ---
tryKey("object", {});
tryKey("function", function named() { return 1; });
tryKey("array", []);

// --- every other primitive is refused ---
tryKey("number", 1);
tryKey("string", "s");
tryKey("boolean", true);
tryKey("null", null);
tryKey("undefined", undefined);
tryKey("bigint", 10n);
tryKey("nan", NaN);

// --- two symbols with the same description are distinct keys ---
const a1 = Symbol("dup");
const a2 = Symbol("dup");
const wm2 = new WeakMap<any, any>();
wm2.set(a1, "first");
wm2.set(a2, "second");
console.log("distinct_descriptions=" + wm2.get(a1) + ":" + wm2.get(a2));

// --- the weak collection constructors accept an iterable of symbol keys ---
const s1 = Symbol("ctor1");
const s2 = Symbol("ctor2");
const wm3 = new WeakMap<any, any>([[s1, 1], [s2, 2]] as any);
console.log("ctor_symbols=" + wm3.get(s1) + ":" + wm3.get(s2));
const ws3 = new WeakSet<any>([s1, s2] as any);
console.log("ctor_set_symbols=" + ws3.has(s1) + ":" + ws3.has(s2));

// --- get/has on an unheldable key answer instead of throwing ---
console.log("get_primitive=" + wm.get(1 as any));
console.log("has_primitive=" + wm.has("nope" as any));
console.log("delete_primitive=" + wm.delete(1 as any));
console.log("wsHas_primitive=" + ws.has(1 as any));
console.log("wsDelete_primitive=" + ws.delete(1 as any));
