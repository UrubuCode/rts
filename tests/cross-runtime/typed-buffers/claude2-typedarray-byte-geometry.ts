// Cross-runtime: the arithmetic tying length, byteLength, byteOffset and
// BYTES_PER_ELEMENT together. Views of different widths over ONE buffer see the
// same bytes at different indices, and subarray keeps the geometry consistent.

const kinds: [string, any][] = [
  ["Int8Array", Int8Array],
  ["Uint8Array", Uint8Array],
  ["Uint8ClampedArray", Uint8ClampedArray],
  ["Int16Array", Int16Array],
  ["Uint16Array", Uint16Array],
  ["Int32Array", Int32Array],
  ["Uint32Array", Uint32Array],
  ["Float32Array", Float32Array],
  ["Float64Array", Float64Array],
  ["BigInt64Array", BigInt64Array],
  ["BigUint64Array", BigUint64Array],
];

// BYTES_PER_ELEMENT is on the constructor AND on the prototype, both frozen.
for (const pair of kinds) {
  const ctor: any = pair[1];
  const d: any = Object.getOwnPropertyDescriptor(ctor, "BYTES_PER_ELEMENT");
  const a = new ctor(3);
  console.log("bpe_" + pair[0] + "=" + ctor.BYTES_PER_ELEMENT + " proto:" + ctor.prototype.BYTES_PER_ELEMENT + " inst:" + a.BYTES_PER_ELEMENT + " len:" + a.length + " bytes:" + a.byteLength + " w:" + d.writable + " e:" + d.enumerable + " c:" + d.configurable);
}
console.log("bpe_on_intrinsic=" + String((Object.getPrototypeOf(Uint8Array) as any).BYTES_PER_ELEMENT) + "," + String((Object.getPrototypeOf(Uint8Array.prototype) as any).BYTES_PER_ELEMENT));

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
};

// One buffer, three widths: byteOffset is in BYTES, length is in ELEMENTS.
const buf = new ArrayBuffer(16);
const asBytes = new Uint8Array(buf);
const asWords = new Uint16Array(buf, 4);
const asLongs = new Uint32Array(buf, 8, 2);
console.log("bytes=" + asBytes.length + "/" + asBytes.byteOffset + "/" + asBytes.byteLength);
console.log("words=" + asWords.length + "/" + asWords.byteOffset + "/" + asWords.byteLength);
console.log("longs=" + asLongs.length + "/" + asLongs.byteOffset + "/" + asLongs.byteLength);
console.log("same_buffer=" + (asBytes.buffer === buf) + "," + (asWords.buffer === buf) + "," + (asLongs.buffer === buf));

// A write through the widest view is visible through the narrowest, at the
// byteOffset the geometry predicts.
asLongs[0] = 0x01020304;
console.log("aliased_bytes=" + Array.from(asBytes.subarray(8, 12)).sort(function (a, b) { return a - b; }).join(","));
console.log("aliased_words=" + (asWords[2] !== 0) + "," + (asWords[3] !== 0));
console.log("byte_sum=" + Array.from(asBytes.subarray(8, 12)).reduce(function (a, b) { return a + b; }, 0));

// subarray recomputes byteOffset from the receiver's own, in elements.
const sub = asWords.subarray(1, 4);
console.log("sub=" + sub.length + "/" + sub.byteOffset + "/" + sub.byteLength + " shares=" + (sub.buffer === buf));
const subOfSub = sub.subarray(1);
console.log("sub_of_sub=" + subOfSub.length + "/" + subOfSub.byteOffset + "/" + subOfSub.byteLength);
console.log("sub_negative=" + (function (): string { const s = asWords.subarray(-2); return s.length + "/" + s.byteOffset; })());
console.log("sub_inverted=" + (function (): string { const s = asWords.subarray(4, 1); return s.length + "/" + s.byteOffset; })());
console.log("sub_past_end=" + (function (): string { const s = asWords.subarray(3, 99); return s.length + "/" + s.byteOffset; })());
console.log("sub_kind=" + sub.constructor.name + " sub_empty_offset=" + asWords.subarray(6, 6).byteOffset);

// slice copies into a fresh buffer, so byteOffset restarts at zero.
const copy = asWords.slice(1, 4);
console.log("slice_geometry=" + copy.length + "/" + copy.byteOffset + "/" + copy.byteLength + " own_buffer=" + (copy.buffer !== buf) + " buffer_len=" + copy.buffer.byteLength);

// The whole-buffer forms.
console.log("infer_length=" + new Uint32Array(buf).length + " infer_from_offset=" + new Uint32Array(buf, 8).length);
console.log("explicit_zero=" + new Uint32Array(buf, 8, 0).length + " byteOffset=" + new Uint32Array(buf, 8, 0).byteOffset);
console.log("misaligned=" + t(function () { return new Uint32Array(buf, 2); }) + " uneven=" + t(function () { return new Uint32Array(new ArrayBuffer(10)); }));
console.log("byte_kind_any_offset=" + new Uint8Array(buf, 3).length + " int16_odd_offset=" + t(function () { return new Int16Array(buf, 3); }));
console.log("empty_buffer_views=" + new Uint32Array(new ArrayBuffer(0)).length + "/" + new DataView(new ArrayBuffer(0)).byteLength);
console.log("bpe_times_length=" + kinds.map(function (pair) {
  const a = new (pair[1] as any)(4);
  return String(a.byteLength === a.length * a.BYTES_PER_ELEMENT);
}).join(","));
