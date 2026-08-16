// Cross-runtime: Math, JSON, Reflect and Atomics are ORDINARY objects, not
// constructors — no [[Call]], no [[Construct]], no .prototype — each carrying a
// non-writable configurable Symbol.toStringTag that names it.

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
};

const namespaces: [string, any][] = [["Math", Math], ["JSON", JSON], ["Reflect", Reflect], ["Atomics", Atomics]];

for (const pair of namespaces) {
  const name = pair[0];
  const ns = pair[1];
  const d: any = Object.getOwnPropertyDescriptor(ns, Symbol.toStringTag);
  console.log("tag_" + name + "=" + ns[Symbol.toStringTag] + " w:" + d.writable + " e:" + d.enumerable + " c:" + d.configurable);
}
for (const pair of namespaces) {
  console.log("brand_" + pair[0] + "=" + Object.prototype.toString.call(pair[1]));
}
for (const pair of namespaces) {
  const ns = pair[1];
  console.log("shape_" + pair[0] + "=type:" + typeof ns + " proto_is_object:" + (Object.getPrototypeOf(ns) === Object.prototype) + " has_prototype:" + ("prototype" in ns) + " enumerable_keys:" + Object.keys(ns).length + " extensible:" + Object.isExtensible(ns));
}
for (const pair of namespaces) {
  console.log("not_callable_" + pair[0] + "=" + t(function () { return (pair[1] as any)(); }) + " not_constructible:" + t(function () { return new (pair[1] as any)(); }));
}
for (const pair of namespaces) {
  const d: any = Object.getOwnPropertyDescriptor(globalThis, pair[0]);
  console.log("global_" + pair[0] + "=w:" + d.writable + " e:" + d.enumerable + " c:" + d.configurable + " same:" + (d.value === pair[1]));
}

// Math's constants are frozen data properties; its functions are ordinary.
console.log("math_constants=" + ["PI", "E", "LN2", "LN10", "LOG2E", "LOG10E", "SQRT2", "SQRT1_2"].map(function (k) {
  const d: any = Object.getOwnPropertyDescriptor(Math, k);
  return k + ":" + (typeof d.value) + d.writable + d.enumerable + d.configurable;
}).join(" "));
console.log("math_pi=" + Math.PI + " sqrt2=" + Math.SQRT2);
console.log("math_fn_lengths=" + ["abs", "max", "min", "hypot", "atan2", "pow", "round", "clz32", "imul", "fround"].map(function (k) {
  const f: any = (Math as any)[k];
  return k + ":" + (typeof f === "function" ? String(f.length) : "absent");
}).join(" "));
console.log("math_no_this=" + t(function () {
  const abs = Math.abs;
  return abs(-3);
}));
console.log("math_fn_not_constructible=" + t(function () { return new (Math.abs as any)(1); }));

// JSON has exactly the two operations, plus the ES2025 raw-JSON pair when present.
console.log("json_members=" + ["parse", "stringify", "rawJSON", "isRawJSON"].map(function (k) { return k + ":" + typeof (JSON as any)[k]; }).join(" "));
console.log("json_lengths=" + JSON.parse.length + "," + JSON.stringify.length);
console.log("json_detached=" + t(function () {
  const stringify = JSON.stringify;
  return stringify({ a: 1 });
}));

// Reflect's thirteen operations, by arity.
const reflectOps = ["apply", "construct", "defineProperty", "deleteProperty", "get", "getOwnPropertyDescriptor", "getPrototypeOf", "has", "isExtensible", "ownKeys", "preventExtensions", "set", "setPrototypeOf"];
console.log("reflect_arity=" + reflectOps.map(function (k) { return (Reflect as any)[k].length; }).join(","));
console.log("reflect_all_functions=" + reflectOps.every(function (k) { return typeof (Reflect as any)[k] === "function"; }));
console.log("reflect_no_apply_alias=" + (Reflect.apply === Function.prototype.apply));
console.log("reflect_get_receiver=" + t(function () {
  const o = { a: 1, get b() { return (this as any).a; } };
  return Reflect.get(o, "b", { a: 99 });
}));

// Atomics' members, and the fact that it is reachable without a shared buffer.
console.log("atomics_members=" + ["add", "and", "compareExchange", "exchange", "isLockFree", "load", "notify", "or", "store", "sub", "wait", "waitAsync", "xor"].map(function (k) { return typeof (Atomics as any)[k]; }).join(","));
console.log("atomics_arity=" + Atomics.load.length + "," + Atomics.store.length + "," + Atomics.compareExchange.length);

// A namespace can be shadowed on the global object and put back, because the
// global binding is writable and configurable while the object itself is not.
console.log("global_writable=" + (function (): string {
  const saved = (globalThis as any).Reflect;
  (globalThis as any).Reflect = 1;
  const changed = typeof (globalThis as any).Reflect;
  (globalThis as any).Reflect = saved;
  return changed + "/" + typeof (globalThis as any).Reflect + "/" + ((globalThis as any).Reflect === saved);
})());
console.log("tag_is_not_writable=" + t(function () {
  return Reflect.set(Math, Symbol.toStringTag, "Nope") + "/" + Math[Symbol.toStringTag];
}));
console.log("tag_is_configurable=" + t(function () {
  const saved: any = Object.getOwnPropertyDescriptor(Math, Symbol.toStringTag);
  Object.defineProperty(Math, Symbol.toStringTag, { value: "Changed", writable: false, enumerable: false, configurable: true });
  const seen = Object.prototype.toString.call(Math);
  Object.defineProperty(Math, Symbol.toStringTag, saved);
  return seen + "/" + Object.prototype.toString.call(Math);
}));
