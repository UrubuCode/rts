// Cross-runtime: what TypedArray.prototype.set accepts as a SOURCE — a plain
// array with holes and getters, an array-like, a string, another kind — and how
// the offset argument is coerced before the length check that raises RangeError.

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
};

// A hole reads as undefined, which becomes 0 in an integer kind and NaN in a
// float one — the ordinary write coercion, applied per element.
console.log("holes_int=" + t(function () { const d = new Uint8Array(3).fill(9); d.set([1, , 3] as any); return d.join(","); }));
console.log("holes_float=" + t(function () { const d = new Float64Array(3).fill(9); d.set([1, , 3] as any); return d.join(","); }));
console.log("undefined_element=" + t(function () { const d = new Uint8Array(2); d.set([undefined, null] as any); return d.join(","); }));
console.log("string_elements=" + t(function () { const d = new Uint8Array(3); d.set(["7", "x", true] as any); return d.join(","); }));
console.log("bool_and_object=" + t(function () { const d = new Int16Array(2); d.set([true, { valueOf: function () { return 5; } }] as any); return d.join(","); }));

// An array-like source reads .length and then each index in ascending order.
const order: string[] = [];
const arrayLike: any = { length: 3 };
Object.defineProperty(arrayLike, 0, { get: function () { order.push("g0"); return 1; }, enumerable: true });
Object.defineProperty(arrayLike, 1, { get: function () { order.push("g1"); return 2; }, enumerable: true });
Object.defineProperty(arrayLike, 2, { get: function () { order.push("g2"); return 3; }, enumerable: true });
console.log("arraylike=" + t(function () { const d = new Uint8Array(3); d.set(arrayLike); return d.join(","); }));
console.log("getter_order=" + order.join(">"));
console.log("missing_indices=" + t(function () { const d = new Uint8Array(3).fill(9); d.set({ length: 3, 1: 5 } as any); return d.join(","); }));
console.log("length_coerced=" + t(function () { const d = new Uint8Array(3).fill(9); d.set({ length: "2", 0: 1, 1: 2 } as any); return d.join(","); }));
console.log("no_length=" + t(function () { const d = new Uint8Array(2).fill(9); d.set({ 0: 1 } as any); return d.join(","); }));
console.log("string_source=" + t(function () { const d = new Uint8Array(3).fill(9); d.set("ab" as any); return d.join(","); }));
console.log("iterable_not_used=" + t(function () {
  const d = new Uint8Array(2).fill(9);
  const src: any = { length: 2, 0: 1, 1: 2 };
  src[Symbol.iterator] = function* () { yield 7; yield 8; };
  d.set(src);
  return d.join(",");
}));
console.log("null_source=" + t(function () { return new Uint8Array(2).set(null as any); }));
console.log("undefined_source=" + t(function () { return new Uint8Array(2).set(undefined as any); }));
console.log("number_source=" + t(function () { const d = new Uint8Array(2).fill(9); d.set(5 as any); return d.join(","); }));
console.log("returns_undefined=" + String(new Uint8Array(2).set([1])));

// Offset: ToIntegerOrInfinity, so a fraction truncates and a negative is refused
// BEFORE the source is read.
console.log("offset_fraction=" + t(function () { const d = new Uint8Array(3); d.set([1], 1.9 as any); return d.join(","); }));
console.log("offset_string=" + t(function () { const d = new Uint8Array(3); d.set([1], "2" as any); return d.join(","); }));
console.log("offset_undefined=" + t(function () { const d = new Uint8Array(3); d.set([1], undefined); return d.join(","); }));
console.log("offset_negative=" + t(function () { return new Uint8Array(3).set([1], -1); }));
console.log("offset_nan=" + t(function () { const d = new Uint8Array(3); d.set([1], NaN as any); return d.join(","); }));
console.log("offset_infinity=" + t(function () { return new Uint8Array(3).set([1], Infinity); }));
console.log("too_long=" + t(function () { return new Uint8Array(3).set([1, 2], 2); }));
console.log("exact_fit=" + t(function () { const d = new Uint8Array(3); d.set([1, 2], 1); return d.join(","); }));
console.log("empty_source_at_end=" + t(function () { const d = new Uint8Array(2); d.set([], 2); return d.join(","); }));
console.log("empty_source_past_end=" + t(function () { return new Uint8Array(2).set([], 3); }));
console.log("range_before_read=" + t(function () {
  let read = 0;
  const src: any = { length: 5 };
  Object.defineProperty(src, 0, { get: function () { read++; return 1; } });
  try {
    new Uint8Array(2).set(src);
  } catch (e: any) {
    return e.constructor.name + "/reads:" + read;
  }
  return "no-throw/reads:" + read;
}));

// A typed-array source converts element by element through the destination kind.
console.log("narrowing=" + t(function () { const d = new Uint8Array(3); d.set(new Float64Array([1.7, -1, 300])); return d.join(","); }));
console.log("widening=" + t(function () { const d = new Float64Array(2); d.set(new Uint8Array([1, 2])); return d.join(","); }));
console.log("clamped_target=" + t(function () { const d = new Uint8ClampedArray(3); d.set(new Float64Array([-5, 2.5, 300])); return d.join(","); }));
console.log("same_buffer_wider=" + t(function () {
  const buf = new ArrayBuffer(8);
  const bytes = new Uint8Array(buf);
  bytes.set([1, 2, 3, 4, 5, 6, 7, 8]);
  const words = new Uint16Array(buf);
  bytes.set(words.subarray(0, 4) as any);
  return bytes.join(",");
}));
console.log("overlapping_same_kind=" + t(function () {
  const a = new Uint8Array([1, 2, 3, 4, 5, 6]);
  a.set(a.subarray(0, 4), 2);
  return a.join(",");
}));
