// Cross-runtime: globalThis as an OBJECT — every global name is a property of it
// and answers the same object read either way, a new property added to it is a
// plain writable-enumerable-configurable one, and the value properties refuse writes.

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
};

const g: any = globalThis;

console.log("self_reference=" + (g.globalThis === globalThis) + "/" + (g.globalThis.globalThis === globalThis));
console.log("self_desc=" + (function (): string {
  const d: any = Object.getOwnPropertyDescriptor(globalThis, "globalThis");
  return "w:" + d.writable + " e:" + d.enumerable + " c:" + d.configurable;
})());
console.log("type=" + typeof globalThis + " tag_is_object_like=" + (Object.prototype.toString.call(globalThis).indexOf("[object ") === 0));

// Reading a global by name and through globalThis is the same object.
console.log("identity_ctors=" + [Object, Array, Function, Promise, Map, Set, Symbol, Proxy, Reflect, Math, JSON, DOMException, TextEncoder].map(function (v: any, i: number) {
  const names = ["Object", "Array", "Function", "Promise", "Map", "Set", "Symbol", "Proxy", "Reflect", "Math", "JSON", "DOMException", "TextEncoder"];
  return String(g[names[i]] === v);
}).join(","));
console.log("identity_functions=" + [["structuredClone", structuredClone], ["queueMicrotask", queueMicrotask], ["setTimeout", setTimeout], ["btoa", btoa], ["encodeURIComponent", encodeURIComponent], ["parseInt", parseInt], ["isNaN", isNaN]].map(function (pair: any) { return String(g[pair[0]] === pair[1]); }).join(","));
console.log("in_operator=" + ["undefined", "NaN", "Infinity", "globalThis", "Math", "Reflect", "Atomics", "structuredClone", "DOMException"].map(function (n) { return String(n in globalThis); }).join(","));
console.log("missing_name=" + ("__definitely_absent" in globalThis) + "/" + typeof g.__definitely_absent + "/" + String(g.__definitely_absent));

// The three value properties are non-writable, so a [[Set]] simply answers false.
console.log("value_props=" + ["undefined", "NaN", "Infinity"].map(function (n) {
  const d: any = Object.getOwnPropertyDescriptor(globalThis, n);
  return n + ":w" + d.writable + "e" + d.enumerable + "c" + d.configurable;
}).join(" "));
console.log("set_undefined=" + Reflect.set(globalThis, "undefined", 1) + " still=" + String(g.undefined));
console.log("set_nan=" + Reflect.set(globalThis, "NaN", 1) + " still=" + String(g.NaN));
console.log("set_infinity=" + Reflect.set(globalThis, "Infinity", 1) + " still=" + String(g.Infinity));
console.log("delete_value_prop=" + Reflect.deleteProperty(globalThis, "NaN") + " still=" + String(g.NaN));
console.log("define_over_value_prop=" + t(function () { return Object.defineProperty(globalThis, "undefined", { value: 1 }); }));

// A property added by assignment is an ordinary one; one added by
// defineProperty keeps exactly the attributes it was given.
console.log("added_by_assignment=" + (function (): string {
  g.__probeA = 7;
  const d: any = Object.getOwnPropertyDescriptor(globalThis, "__probeA");
  const shape = d.value + "/w:" + d.writable + " e:" + d.enumerable + " c:" + d.configurable;
  delete g.__probeA;
  return shape + "/gone:" + !("__probeA" in globalThis);
})());
console.log("added_by_define=" + (function (): string {
  Object.defineProperty(globalThis, "__probeB", { value: 1, configurable: true });
  const d: any = Object.getOwnPropertyDescriptor(globalThis, "__probeB");
  const shape = "w:" + d.writable + " e:" + d.enumerable + " c:" + d.configurable;
  delete g.__probeB;
  return shape;
})());
console.log("added_accessor=" + (function (): string {
  let stored = 0;
  Object.defineProperty(globalThis, "__probeC", {
    get: function () { return stored + 1; },
    set: function (v: number) { stored = v * 2; },
    configurable: true,
  });
  g.__probeC = 5;
  const seen = g.__probeC;
  delete g.__probeC;
  return seen + "/" + stored;
})());

// A constructor binding is writable and configurable: it can be replaced,
// deleted and put back, which is how a polyfill installs itself.
console.log("ctor_desc=" + ["Map", "Promise", "WeakRef", "Proxy"].map(function (n) {
  const d: any = Object.getOwnPropertyDescriptor(globalThis, n);
  return n + ":w" + d.writable + "e" + d.enumerable + "c" + d.configurable;
}).join(" "));
console.log("replace_and_restore=" + (function (): string {
  const saved = g.WeakRef;
  g.WeakRef = 1;
  const replaced = typeof g.WeakRef;
  g.WeakRef = saved;
  return replaced + "/" + typeof g.WeakRef + "/" + (g.WeakRef === saved);
})());
console.log("delete_and_restore=" + (function (): string {
  const saved = g.WeakRef;
  const deleted = delete g.WeakRef;
  const gone = "WeakRef" in globalThis;
  g.WeakRef = saved;
  return deleted + "/" + gone + "/" + typeof g.WeakRef;
})());
console.log("shadow_does_not_touch_object=" + (function (): string {
  const savedMap = g.Map;
  const instance = new Map([["k", 1]]);
  g.Map = null;
  const stillWorks = instance.get("k");
  g.Map = savedMap;
  return String(stillWorks) + "/" + (instance instanceof Map);
})());

// Enumerability: the ECMAScript globals are all non-enumerable, so a for-in over
// the global object sees user properties and the web-platform additions.
console.log("enumerable_flags=" + ["Object", "Math", "undefined", "NaN", "globalThis", "structuredClone"].map(function (n) {
  const d: any = Object.getOwnPropertyDescriptor(globalThis, n);
  return String(d.enumerable);
}).join(","));
console.log("keys_see_user_property=" + (function (): string {
  g.__probeD = 1;
  const inKeys = Object.keys(globalThis).indexOf("__probeD") >= 0;
  let inForIn = false;
  for (const k in globalThis) {
    if (k === "__probeD") inForIn = true;
  }
  delete g.__probeD;
  return inKeys + "/" + inForIn;
})());
console.log("ownKeys_has_symbols=" + (Object.getOwnPropertySymbols(globalThis).length >= 0) + " extensible=" + Object.isExtensible(globalThis));
console.log("prototype_of_ctor_matches=" + (Object.getPrototypeOf(new Map()) === g.Map.prototype) + "/" + (Object.getPrototypeOf(g) !== null));
