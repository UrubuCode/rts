// Cross-runtime: TypedArray.prototype.set from an array-like PLAIN OBJECT, the
// RangeError that is raised before anything is written, offset coercion, and
// the constructor forms that share the conversion machinery.

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name + ":" + e.message;
  }
};

console.log("plain_array=" + t(function () { const d = new Uint8Array(3); d.set([1, 2], 1); return d.join(); }));
console.log("plain_object=" + t(function () { const d = new Uint8Array(3); d.set({ length: 2, 0: 5, 1: 6 } as any, 0); return d.join(); }));
console.log("plain_object_offset=" + t(function () { const d = new Uint8Array(3).fill(9); d.set({ length: 2, 0: 5, 1: 6 } as any, 1); return d.join(); }));
console.log("no_length=" + t(function () { const d = new Uint8Array(3).fill(9); d.set({ 0: 1, 1: 2 } as any); return d.join(); }));
console.log("overflow_object=" + t(function () { const d = new Uint8Array(3).fill(9); d.set({ length: 3, 0: 1, 1: 2, 2: 3 } as any, 1); return d.join(); }));
console.log("overflow_untouched=" + t(function () {
  const d = new Uint8Array(3).fill(9);
  try { d.set([1, 2, 3, 4] as any); } catch (e) {}
  return d.join();
}));
console.log("strings_int=" + t(function () { const d = new Uint8Array(3).fill(9); d.set(["5", "", "x"] as any); return d.join(); }));
console.log("strings_float=" + t(function () { const d = new Float64Array(3).fill(9); d.set(["5", "", "x"] as any); return d.join(); }));
console.log("negative_offset=" + t(function () { return new Uint8Array(3).set([1], -1); }));
console.log("int8_wrap=" + t(function () { const d = new Int8Array(3); d.set([200, -200, 1.9] as any); return d.join(); }));
console.log("clamped=" + t(function () { const d = new Uint8ClampedArray(3); d.set([200, -200, 2.5] as any); return d.join(); }));
console.log("float64_fraction=" + t(function () { const d = new Float64Array(2); d.set({ length: 2, 0: 1.5, 1: "2.5" } as any); return d.join(); }));
console.log("float_source_into_int=" + t(function () { const d = new Uint8Array(3); d.set(new Float64Array([1.7, -1, 300])); return d.join(); }));
console.log("view_overflow=" + t(function () { return new Uint8Array(2).set(new Uint8Array(3)); }));
console.log("view_negative_offset=" + t(function () { return new Uint8Array(2).set(new Uint8Array(1), -1); }));
console.log("getter_throws=" + t(function () { const d = new Uint8Array(3).fill(9); d.set({ length: 2, 0: 1, get 1() { throw new Error("boom"); } } as any); return d.join(); }));
console.log("getter_partial=" + t(function () {
  const d = new Uint8Array(3).fill(9);
  try { d.set({ length: 2, 0: 1, get 1() { throw new Error("boom"); } } as any); } catch (e) {}
  return d.join();
}));
console.log("valueof_throws=" + t(function () { const d = new Uint8Array(3).fill(9); d.set({ length: 2, 0: 1, 1: { valueOf() { throw new TypeError("v"); } } } as any); return d.join(); }));
console.log("null_source=" + t(function () { return new Uint8Array(3).set(null as any); }));
console.log("string_source=" + t(function () { const d = new Uint8Array(3).fill(9); d.set("12" as any); return d.join(); }));
console.log("negative_length=" + t(function () { const d = new Uint8Array(3).fill(9); d.set({ length: -1 } as any); return d.join(); }));
console.log("fraction_offset=" + t(function () { const d = new Uint8Array(3).fill(9); d.set({ length: 1, 0: 7 } as any, 2.9 as any); return d.join(); }));
console.log("bigint_target=" + t(function () { const d = new BigInt64Array(2); d.set([1n, 2n] as any); return d.join(); }));
console.log("bigint_number_array=" + t(function () { return new BigInt64Array(2).set([1, 2] as any); }));
console.log("bigint_mix=" + t(function () { return new BigInt64Array(2).set(new Uint8Array(1)); }));

// The constructor shares the conversion code and must not change.
console.log("ctor_arraylike=" + t(function () { return new Uint8Array({ length: 2, 0: 5, 1: 6 } as any).join(); }));
console.log("ctor_array=" + t(function () { return new Uint8Array([1, 2, 3]).join(); }));
console.log("ctor_from_view=" + t(function () { return new Float64Array(new Uint8Array([1, 2])).join(); }));
console.log("ctor_length=" + t(function () { return new Uint8Array(3).join(); }));
