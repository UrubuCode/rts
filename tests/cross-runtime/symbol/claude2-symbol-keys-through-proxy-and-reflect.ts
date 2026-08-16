// Cross-runtime: a symbol travels through the meta-object protocol as a KEY in
// its own right. Every Proxy trap receives it as a symbol, never as a string,
// and the ownKeys invariants count it exactly as they count a string key.

const s1 = Symbol("alpha");
const s2 = Symbol.for("claude2-proxy-beta");
const traps: string[] = [];

function keyOf(k: any): string {
  return typeof k === "symbol" ? "sym(" + String(k.description) + ")" : "str(" + String(k) + ")";
}

const target: any = { plain: 1 };
target[s1] = "one";

const p: any = new Proxy(target, {
  get(t: any, k: any, r: any) { traps.push("get " + keyOf(k)); return Reflect.get(t, k, r); },
  set(t: any, k: any, v: any, r: any) { traps.push("set " + keyOf(k)); return Reflect.set(t, k, v, r); },
  has(t: any, k: any) { traps.push("has " + keyOf(k)); return Reflect.has(t, k); },
  deleteProperty(t: any, k: any) { traps.push("delete " + keyOf(k)); return Reflect.deleteProperty(t, k); },
  defineProperty(t: any, k: any, d: any) { traps.push("define " + keyOf(k)); return Reflect.defineProperty(t, k, d); },
  getOwnPropertyDescriptor(t: any, k: any) { traps.push("gopd " + keyOf(k)); return Reflect.getOwnPropertyDescriptor(t, k); },
  ownKeys(t: any) { traps.push("ownKeys"); return Reflect.ownKeys(t); },
});

// --- each operation reaches the trap with the symbol intact ---
console.log("read=" + p[s1]);
console.log("write=" + (p[s2] = "two"));
console.log("has=" + (s1 in p));
console.log("has_missing=" + (Symbol("nope") in p));
console.log("delete=" + Reflect.deleteProperty(p, s2));
console.log("define=" + Reflect.defineProperty(p, s2, { value: "redefined", enumerable: true, configurable: true, writable: true }));
console.log("descriptor=" + (Object.getOwnPropertyDescriptor(p, s2) as any).value);
console.log("traps=" + traps.join("|"));

// --- ownKeys hands symbols and strings back in one list ---
traps.length = 0;
console.log("ownKeys=" + Reflect.ownKeys(p).map(keyOf).join(","));
console.log("getOwnPropertySymbols=" + Object.getOwnPropertySymbols(p).map(keyOf).join(","));
console.log("getOwnPropertyNames=" + Object.getOwnPropertyNames(p).join(","));
console.log("Object_keys=" + Object.keys(p).join(","));
console.log("ownKeys_traps=" + traps.join("|"));

// --- Reflect passes a symbol receiver through untouched ---
console.log("reflect_get=" + Reflect.get(p, s1));
console.log("reflect_set=" + Reflect.set(p, s1, "one_prime"));
console.log("reflect_get_after=" + Reflect.get(p, s1));
console.log("reflect_has=" + Reflect.has(p, s1));
console.log("reflect_gopd_flags=" + JSON.stringify(Reflect.getOwnPropertyDescriptor(p, s1)));

// --- a trap may answer differently per key kind ---
const selective: any = new Proxy({}, {
  get(_t: any, k: any) { return typeof k === "symbol" ? "SYMBOL" : "STRING"; },
  has(_t: any, k: any) { return typeof k === "symbol"; },
  ownKeys() { return [Symbol.for("claude2-only-symbol")]; },
  getOwnPropertyDescriptor() { return { value: 1, enumerable: true, configurable: true }; },
});
console.log("selective_symbol=" + selective[s1]);
console.log("selective_string=" + selective.anything);
console.log("selective_has_symbol=" + (s1 in selective));
console.log("selective_has_string=" + ("x" in selective));
console.log("selective_ownKeys=" + Reflect.ownKeys(selective).map(keyOf).join(","));
console.log("selective_keys=" + Object.keys(selective).length);
console.log("selective_symbols=" + Object.getOwnPropertySymbols(selective).map(keyOf).join(","));

// --- ownKeys must not drop a NON-CONFIGURABLE symbol key of the target ---
const fixed: any = {};
Object.defineProperty(fixed, s1, { value: "fixed", configurable: false, enumerable: false, writable: false });
const dropping: any = new Proxy(fixed, { ownKeys() { return []; } });
try { Reflect.ownKeys(dropping); console.log("drop_nonconfigurable=no_throw"); }
catch (e: any) { console.log("drop_nonconfigurable=" + e.constructor.name); }

// --- and it must not report a duplicate ---
const dup: any = new Proxy({}, { ownKeys() { return [s1, s1]; }, getOwnPropertyDescriptor() { return { value: 1, enumerable: true, configurable: true }; } });
try { console.log("duplicate=" + Reflect.ownKeys(dup).length); }
catch (e: any) { console.log("duplicate=" + e.constructor.name); }

// --- ownKeys must return a list of property keys only ---
const badKeys: any = new Proxy({}, { ownKeys() { return [1 as any]; } });
try { Reflect.ownKeys(badKeys); console.log("number_key=no_throw"); }
catch (e: any) { console.log("number_key=" + e.constructor.name); }

// --- for-in skips symbols even when the trap offers them ---
const enumerable: any = new Proxy({ str: 1 }, {
  ownKeys(t: any) { return [s1, ...Reflect.ownKeys(t)]; },
  getOwnPropertyDescriptor(t: any, k: any) {
    return typeof k === "symbol" ? { value: 1, enumerable: true, configurable: true } : Reflect.getOwnPropertyDescriptor(t, k);
  },
});
let forIn = "";
for (const k in enumerable) forIn += keyOf(k) + ",";
console.log("for_in=" + forIn);
console.log("spread_keys=" + Reflect.ownKeys({ ...enumerable }).map(keyOf).join(","));
console.log("assign_keys=" + Reflect.ownKeys(Object.assign({}, enumerable)).map(keyOf).join(","));

// --- a symbol key never collides with the string that describes it ---
const collide: any = {};
collide[s1] = "symbol_value";
collide[String(s1)] = "string_value";
collide["alpha"] = "plain";
console.log("no_collision=" + collide[s1] + ":" + collide[String(s1)] + ":" + collide.alpha);
console.log("collide_count=" + Reflect.ownKeys(collide).length);

// --- destructuring and optional chaining accept a computed symbol key ---
const src: any = { [s1]: "destructured" };
const { [s1]: pulled, ...rest } = src;
console.log("destructured=" + pulled + ":rest_keys=" + Reflect.ownKeys(rest).length);
console.log("optional_chain=" + String(src?.[s1]) + ":" + String((null as any)?.[s1]));

// --- a class may key a member, a static and an accessor with a symbol ---
class Keyed {
  static [s2] = "static";
  [s1] = "field";
  get [Symbol.toStringTag]() { return "Keyed"; }
  [s2](): string { return "method"; }
}
const inst: any = new Keyed();
console.log("class_field=" + inst[s1]);
console.log("class_method=" + inst[s2]());
console.log("class_static=" + (Keyed as any)[s2]);
console.log("class_tag=" + Object.prototype.toString.call(inst));
console.log("class_instance_symbols=" + Object.getOwnPropertySymbols(inst).map(keyOf).join(","));
console.log("class_proto_symbols=" + Object.getOwnPropertySymbols(Keyed.prototype).map(keyOf).sort().join(","));
