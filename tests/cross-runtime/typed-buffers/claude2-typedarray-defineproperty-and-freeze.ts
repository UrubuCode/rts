// Cross-runtime: an integer-indexed property of a typed array can only ever be a
// writable-enumerable-configurable DATA property, so defineProperty refuses any
// other descriptor — and that is why freezing a NON-EMPTY typed array throws.

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
};

const ta = new Uint8Array([1, 2, 3]);

// The descriptor an index reports is always the full data shape.
const d0: any = Object.getOwnPropertyDescriptor(ta, 0);
console.log("index_desc=" + d0.value + " w:" + d0.writable + " e:" + d0.enumerable + " c:" + d0.configurable);
console.log("oob_desc=" + String(Object.getOwnPropertyDescriptor(ta, 9)));
console.log("length_desc=" + String(Object.getOwnPropertyDescriptor(ta, "length")));
console.log("length_on_proto=" + (typeof (Object.getOwnPropertyDescriptor(Object.getPrototypeOf(Object.getPrototypeOf(ta)), "length") as any).get));

// A full data descriptor is accepted and writes through the coercion.
console.log("full_data=" + t(function () {
  Object.defineProperty(ta, 0, { value: 300, writable: true, enumerable: true, configurable: true });
  return ta[0];
}));
// A value-only descriptor keeps the existing attributes, which already match.
console.log("value_only=" + t(function () {
  Object.defineProperty(ta, 1, { value: 7 });
  return ta[1];
}));
console.log("writable_false=" + t(function () { return Object.defineProperty(ta, 0, { value: 1, writable: false }); }));
console.log("enumerable_false=" + t(function () { return Object.defineProperty(ta, 0, { value: 1, enumerable: false }); }));
console.log("configurable_false=" + t(function () { return Object.defineProperty(ta, 0, { value: 1, configurable: false }); }));
console.log("getter=" + t(function () { return Object.defineProperty(ta, 0, { get: function () { return 1; } }); }));
console.log("setter=" + t(function () { return Object.defineProperty(ta, 0, { set: function () { return; } }); }));
console.log("empty_desc=" + t(function () { Object.defineProperty(ta, 2, {}); return ta[2]; }));
console.log("out_of_range=" + t(function () { return Object.defineProperty(ta, 9, { value: 1, writable: true, enumerable: true, configurable: true }); }));
console.log("negative_index=" + t(function () { return Object.defineProperty(ta, -1, { value: 1, writable: true, enumerable: true, configurable: true }); }));
console.log("non_canonical=" + t(function () { return Object.defineProperty(ta, "1.5", { value: 1, writable: true, enumerable: true, configurable: true }); }));

// A plain string key is an ORDINARY property and takes any descriptor.
console.log("string_key=" + t(function () {
  Object.defineProperty(ta, "tag", { value: "x", configurable: true });
  const d: any = Object.getOwnPropertyDescriptor(ta, "tag");
  return (ta as any).tag + " w:" + d.writable + " e:" + d.enumerable + " c:" + d.configurable;
}));
console.log("string_accessor=" + t(function () {
  Object.defineProperty(ta, "acc", { get: function () { return 5; }, configurable: true });
  return (ta as any).acc;
}));
console.log("keys_with_extra=" + Object.keys(ta).join(","));
console.log("ownKeys_with_extra=" + Reflect.ownKeys(ta).join(","));
console.log("forin=" + (function (): string {
  const out: string[] = [];
  for (const k in ta) out.push(k);
  return out.join(",");
})());

// Integrity levels. preventExtensions succeeds; freeze needs every index to
// become non-configurable, which an integer-indexed property cannot be.
console.log("freeze_nonempty=" + t(function () { return Object.freeze(new Uint8Array(1)); }));
console.log("freeze_empty=" + t(function () { return Object.isFrozen(Object.freeze(new Uint8Array(0))); }));
console.log("frozen_before=" + Object.isFrozen(new Uint8Array(1)) + "," + Object.isFrozen(new Uint8Array(0)));
console.log("prevent_extensions=" + t(function () {
  const a = new Uint8Array([4]);
  Object.preventExtensions(a);
  return Object.isExtensible(a) + "/" + a[0] + "/" + Reflect.set(a, "0", 8) + "/" + a[0];
}));
console.log("extend_after_prevent=" + t(function () {
  const a: any = new Uint8Array(1);
  Object.preventExtensions(a);
  return Reflect.set(a, "extra", 1) + "/" + String(a.extra);
}));
console.log("delete_index=" + t(function () {
  const a = new Uint8Array([5]);
  return Reflect.deleteProperty(a, "0") + "/" + a[0] + "/" + Reflect.deleteProperty(a, "9");
}));
console.log("delete_string=" + t(function () {
  const a: any = new Uint8Array(1);
  a.mark = 1;
  return Reflect.deleteProperty(a, "mark") + "/" + String(a.mark);
}));
console.log("defineProperty_returns=" + t(function () {
  const a = new Uint8Array(1);
  return Reflect.defineProperty(a, 0, { value: 2, writable: true, enumerable: true, configurable: true }) + "/" + Reflect.defineProperty(a, 5, { value: 2 });
}));
