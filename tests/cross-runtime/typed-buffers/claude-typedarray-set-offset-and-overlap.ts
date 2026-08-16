// Cross-runtime: TypedArray.prototype.set — the offset argument, conversion
// when source and destination hold different element kinds, an overlapping
// source over the same buffer, and the RangeError that guards the end.

const dst = new Uint8Array(6);
dst.set([1, 2, 3]);
console.log("set_head=" + Array.from(dst).join(","));
dst.set([9, 9], 4);
console.log("set_offset=" + Array.from(dst).join(","));
console.log("set_returns=" + String(dst.set([0], 0)));

// The source is coerced element by element, exactly as a write would be.
const narrowed = new Uint8Array(4);
narrowed.set(new Float64Array([1.7, -2.9, 300.5, NaN]));
console.log("float_into_u8=" + Array.from(narrowed).join(","));
const clamped = new Uint8ClampedArray(4);
clamped.set(new Float32Array([-1.5, 2.5, 3.5, 400]));
console.log("float_into_clamped=" + Array.from(clamped).join(","));
const widened = new Int32Array(3);
widened.set(new Int8Array([-1, 127, -128]));
console.log("i8_into_i32=" + Array.from(widened).join(","));
const backToI8 = new Int8Array(3);
backToI8.set(new Int32Array([255, 256, -1]));
console.log("i32_into_i8=" + Array.from(backToI8).join(","));

// A plain array, an array-like and an iterable-less object all work.
const fromPlain = new Uint8Array(4);
fromPlain.set(["1", true, null, undefined] as any);
console.log("from_plain=" + Array.from(fromPlain).join(","));
const fromArrayLike = new Uint8Array(3);
fromArrayLike.set({ length: 2, 0: 7, 1: 8 } as any);
console.log("from_arraylike=" + Array.from(fromArrayLike).join(","));
const fromEmpty = new Uint8Array(2).fill(5);
fromEmpty.set([] as any, 1);
console.log("from_empty=" + Array.from(fromEmpty).join(","));

// Overlapping source and destination in the SAME buffer, same width.
const a = new Uint8Array([1, 2, 3, 4, 5, 6]);
a.set(a.subarray(0, 4), 2);
console.log("overlap_forward=" + Array.from(a).join(","));
const b = new Uint8Array([1, 2, 3, 4, 5, 6]);
b.set(b.subarray(2, 6), 0);
console.log("overlap_backward=" + Array.from(b).join(","));

// Overlapping ranges of a wider element kind over one buffer.
const buf = new ArrayBuffer(8);
const asWords = new Uint16Array(buf);
asWords.set([1, 2, 3, 4]);
asWords.set(asWords.subarray(1, 4), 0);
console.log("overlap_words=" + Array.from(asWords).join(","));

// A narrow view of the same buffer written from the wide view: the VALUES are
// converted, so the answer does not depend on the machine's byte order.
const buf2 = new ArrayBuffer(4);
const w2 = new Uint16Array(buf2, 0, 2);
const b2 = new Uint8Array(buf2, 0, 2);
w2.set([258, 260]);
b2.set(w2.subarray(0, 2) as any, 0);
console.log("wide_into_narrow_same_buffer=" + Array.from(b2).join(","));

// RangeError guards the far end; the destination is untouched when it throws.
const guard = new Uint8Array([1, 2, 3]);
try {
  guard.set([9, 9], 2);
  console.log("past_end=no-throw");
} catch (e: any) {
  console.log("past_end=" + e.constructor.name);
}
console.log("guard_untouched=" + Array.from(guard).join(","));
try {
  guard.set([9], -1);
  console.log("negative_offset=no-throw");
} catch (e: any) {
  console.log("negative_offset=" + e.constructor.name);
}
try {
  guard.set([9], 3);
  console.log("offset_at_end=no-throw");
} catch (e: any) {
  console.log("offset_at_end=" + e.constructor.name);
}
console.log("empty_at_end=" + (function (): string {
  try {
    guard.set([], 3);
    return "no-throw";
  } catch (e: any) {
    return e.constructor.name;
  }
})());
try {
  guard.set(new Uint8Array(4));
  console.log("source_too_long=no-throw");
} catch (e: any) {
  console.log("source_too_long=" + e.constructor.name);
}

// A fractional offset truncates toward zero rather than throwing.
const frac = new Uint8Array(4);
frac.set([7], 1.9);
console.log("fractional_offset=" + Array.from(frac).join(","));
try {
  new Uint8Array(4).set([1], NaN);
  console.log("nan_offset=no-throw");
} catch (e: any) {
  console.log("nan_offset=" + e.constructor.name);
}

// A Number source cannot be written into a BigInt destination.
try {
  new BigInt64Array(2).set([1, 2] as any);
  console.log("number_into_bigint=no-throw");
} catch (e: any) {
  console.log("number_into_bigint=" + e.constructor.name);
}
const big = new BigUint64Array(2);
big.set(new BigInt64Array([-1n, 2n]));
console.log("bigint_into_biguint=" + big[0] + "," + big[1]);
