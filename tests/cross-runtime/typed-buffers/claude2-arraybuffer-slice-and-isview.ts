// Cross-runtime: ArrayBuffer.prototype.slice — a COPY whose range is clamped the
// way Array#slice clamps, built through Symbol.species — beside ArrayBuffer.isView,
// which answers for views only and never for the buffer itself.

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
};

const buf = new ArrayBuffer(8);
new Uint8Array(buf).set([1, 2, 3, 4, 5, 6, 7, 8]);
const bytesOf = function (b: ArrayBuffer): string {
  return Array.from(new Uint8Array(b)).join(",");
};

console.log("slice_range=" + bytesOf(buf.slice(2, 5)));
console.log("slice_from=" + bytesOf(buf.slice(6)));
console.log("slice_all=" + bytesOf(buf.slice(0)) + " len=" + buf.slice(0).byteLength);
console.log("slice_negative_start=" + bytesOf(buf.slice(-3)));
console.log("slice_negative_end=" + bytesOf(buf.slice(1, -5)));
console.log("slice_both_negative=" + bytesOf(buf.slice(-4, -2)));
console.log("slice_inverted=" + buf.slice(5, 2).byteLength);
console.log("slice_past_end=" + bytesOf(buf.slice(6, 99)));
console.log("slice_before_start=" + bytesOf(buf.slice(-99, 2)));
console.log("slice_empty=" + buf.slice(3, 3).byteLength);
console.log("slice_undefined_end=" + buf.slice(2, undefined).byteLength);
console.log("slice_fraction=" + buf.slice(1.9, 3.9).byteLength + " values=" + bytesOf(buf.slice(1.9, 3.9)));
console.log("slice_string=" + buf.slice("2" as any).byteLength);
console.log("slice_nan=" + buf.slice(NaN as any, 2).byteLength);
console.log("slice_no_args=" + buf.slice().byteLength);
console.log("slice_is_copy=" + (function (): string {
  const c = buf.slice(0, 2);
  new Uint8Array(c)[0] = 99;
  return new Uint8Array(buf)[0] + "/" + (c === buf);
})());
console.log("slice_kind=" + (buf.slice(0) instanceof ArrayBuffer) + " tag=" + Object.prototype.toString.call(buf.slice(0)));

// Species decides the constructor the copy is built with.
console.log("species_default=" + (ArrayBuffer[Symbol.species] === ArrayBuffer));
console.log("species_desc=" + (function (): string {
  const d: any = Object.getOwnPropertyDescriptor(ArrayBuffer, Symbol.species);
  return typeof d.get + "/" + String(d.set) + "/" + d.configurable;
})());
console.log("subclass_slice=" + t(function () {
  class Sub extends ArrayBuffer {}
  return new Sub(4).slice(0).constructor.name;
}));
console.log("species_override=" + t(function () {
  class Sub extends ArrayBuffer {
    static get [Symbol.species]() { return ArrayBuffer; }
  }
  return new Sub(4).slice(0).constructor.name;
}));
console.log("species_bad=" + t(function () {
  class Sub extends ArrayBuffer {
    static get [Symbol.species]() { return Array as any; }
  }
  return new Sub(4).slice(0);
}));
console.log("slice_wrong_receiver=" + t(function () { return (ArrayBuffer.prototype.slice as any).call(new Uint8Array(4), 0); }));
console.log("slice_detached=" + t(function () { const b = new ArrayBuffer(4); b.transfer(); return b.slice(0); }));
console.log("detached_byteLength=" + t(function () { const b = new ArrayBuffer(4); b.transfer(); return b.byteLength; }));

// Slicing a resizable buffer answers a plain, fixed one.
console.log("slice_of_resizable=" + t(function () {
  const b = new ArrayBuffer(4, { maxByteLength: 8 });
  const c = b.slice(0, 2);
  return c.byteLength + "/" + c.resizable + "/" + c.maxByteLength;
}));

// isView: every typed array kind and DataView, nothing else.
const candidates: [string, any][] = [
  ["Uint8Array", new Uint8Array(1)],
  ["Float64Array", new Float64Array(1)],
  ["BigInt64Array", new BigInt64Array(1)],
  ["Uint8ClampedArray", new Uint8ClampedArray(1)],
  ["subarray", new Uint8Array(2).subarray(1)],
  ["DataView", new DataView(new ArrayBuffer(4))],
  ["ArrayBuffer", new ArrayBuffer(4)],
  ["Array", [1, 2]],
  ["null", null],
  ["undefined", undefined],
  ["number", 5],
  ["object", { byteLength: 4 }],
  ["proto", Uint8Array.prototype],
  ["detached_view", (function () { const b = new ArrayBuffer(2); const v = new Uint8Array(b); b.transfer(); return v; })()],
];
for (const pair of candidates) {
  console.log("isView_" + pair[0] + "=" + ArrayBuffer.isView(pair[1]));
}
console.log("isView_no_arg=" + t(function () { return (ArrayBuffer.isView as any)(); }));
console.log("isView_length=" + ArrayBuffer.isView.length + " zero_buffer=" + new ArrayBuffer(0).byteLength);
