// Cross-runtime: structuredClone rebuilds an object from its own ENUMERABLE
// string-keyed properties. A getter is called once and flattened to a data
// property; a non-enumerable one, a symbol key and the prototype are all dropped.

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name + "/" + e.name;
  }
};

const src: any = { plain: 1 };
Object.defineProperty(src, "getter", { get: function () { return 42; }, enumerable: true, configurable: true });
Object.defineProperty(src, "hiddenData", { value: 9, enumerable: false, configurable: true });
Object.defineProperty(src, "hiddenGetter", { get: function () { return 8; }, enumerable: false, configurable: true });
Object.defineProperty(src, "readOnly", { value: 3, writable: false, enumerable: true, configurable: false });
src[Symbol("sym")] = "s";
src[Symbol.iterator] = function () { return; };

const clone: any = structuredClone(src);
console.log("values=" + clone.plain + "," + clone.getter + "," + clone.readOnly);
console.log("getter_flattened=" + JSON.stringify(Object.getOwnPropertyDescriptor(clone, "getter")));
console.log("readonly_flattened=" + JSON.stringify(Object.getOwnPropertyDescriptor(clone, "readOnly")));
console.log("non_enumerable_dropped=" + ("hiddenData" in clone) + "," + ("hiddenGetter" in clone));
console.log("symbols_dropped=" + Object.getOwnPropertySymbols(clone).length + " well_known=" + (Symbol.iterator in clone));
console.log("keys=" + Object.keys(clone).sort().join(","));
console.log("proto=" + (Object.getPrototypeOf(clone) === Object.prototype));

// The getter runs exactly once, on the SOURCE, during the clone.
console.log("getter_call_count=" + (function (): string {
  let calls = 0;
  const o: any = { get v() { calls++; return 1; } };
  const c: any = structuredClone(o);
  const read = c.v + c.v;
  return calls + "/" + read;
})());
console.log("throwing_getter=" + t(function () {
  return structuredClone({ get v() { throw new RangeError("no"); } });
}));
console.log("getter_sees_source=" + (function (): string {
  const o: any = { n: 5, get double() { return this.n * 2; } };
  const c: any = structuredClone(o);
  c.n = 100;
  return c.double + "";
})());

// A class instance loses its prototype: methods go, own fields stay.
console.log("class_instance=" + t(function () {
  class Point {
    x = 1;
    y = 2;
    sum() { return this.x + this.y; }
  }
  const c: any = structuredClone(new Point());
  return c.constructor.name + "/" + c.x + "," + c.y + "/" + typeof c.sum + "/" + (c instanceof Point);
}));
console.log("null_prototype_source=" + t(function () {
  const o = Object.create(null);
  o.k = 1;
  const c: any = structuredClone(o);
  return c.k + "/" + (Object.getPrototypeOf(c) === Object.prototype);
}));
console.log("accessor_only_object=" + t(function () {
  const o = { get a() { return 1; }, set a(_v: number) { return; } };
  const c: any = structuredClone(o);
  const d: any = Object.getOwnPropertyDescriptor(c, "a");
  return c.a + "/" + ("value" in d) + "/" + String(d.get);
}));

// Arrays: extra string properties survive, holes do not stay holes.
console.log("array_extra_props=" + t(function () {
  const a: any = [1, 2];
  a.tag = "x";
  const c: any = structuredClone(a);
  return Array.isArray(c) + "/" + c.length + "/" + c.tag + "/" + JSON.stringify(c);
}));
console.log("sparse_array=" + t(function () {
  const a = [1, , 3];
  const c: any = structuredClone(a);
  return c.length + "/" + (1 in c) + "/" + String(c[1]) + "/" + JSON.stringify(c);
}));
console.log("array_prototype=" + t(function () {
  const c: any = structuredClone([1]);
  return (Object.getPrototypeOf(c) === Array.prototype) + "/" + (c instanceof Array);
}));
console.log("nested_shapes=" + t(function () {
  const o: any = { inner: { get g() { return 7; } } };
  const c: any = structuredClone(o);
  const d: any = Object.getOwnPropertyDescriptor(c.inner, "g");
  return c.inner.g + "/" + d.writable + d.enumerable + d.configurable;
}));

// Map and Set keep insertion order and preserve key identity across the clone.
console.log("map_key_identity=" + t(function () {
  const key = { id: 1 };
  const m = new Map<any, any>([[key, key], ["s", 2]]);
  const c: any = structuredClone(m);
  const first = Array.from(c.keys())[0];
  return (first === c.get(first)) + "/" + (first === key) + "/" + c.size + "/" + Array.from(c.keys()).map(function (k: any) { return typeof k; }).join(",");
}));
console.log("map_order=" + t(function () {
  const m = new Map([["b", 1], ["a", 2], ["c", 3]]);
  return Array.from(structuredClone(m).keys()).join(",");
}));
console.log("set_identity=" + t(function () {
  const shared = { v: 1 };
  const s = new Set([shared, shared, { v: 1 }]);
  const c: any = structuredClone(s);
  return s.size + "/" + c.size;
}));
console.log("boxed_flattening=" + t(function () {
  const b: any = new Number(5);
  b.extra = 1;
  const c: any = structuredClone(b);
  return typeof c + "/" + (c instanceof Number) + "/" + c.valueOf() + "/" + String(c.extra);
}));
console.log("frozen_source=" + t(function () {
  const c: any = structuredClone(Object.freeze({ a: 1 }));
  return Object.isFrozen(c) + "/" + Object.isExtensible(c);
}));
