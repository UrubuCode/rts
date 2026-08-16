// Cross-runtime: buffer/byteLength/byteOffset/length live on %TypedArray%.prototype
// as GETTERS, not as own data properties, so the receiver decides. A foreign
// receiver is a TypeError — except Symbol.toStringTag, which answers undefined.

const proto: any = Object.getPrototypeOf(Uint8Array.prototype);
const getterOf = function (key: any): any {
  return (Object.getOwnPropertyDescriptor(proto, key) as any).get;
};

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
};

const view = new Int16Array(new ArrayBuffer(16), 4, 3);

// The four geometry accessors are inherited, never own.
for (const key of ["buffer", "byteLength", "byteOffset", "length"]) {
  const own = Object.getOwnPropertyDescriptor(view, key);
  const d: any = Object.getOwnPropertyDescriptor(proto, key);
  console.log("shape_" + key + "=own:" + String(own) + " get:" + typeof d.get + " set:" + String(d.set) + " e:" + d.enumerable + " c:" + d.configurable);
}
console.log("values=" + view.byteLength + "," + view.byteOffset + "," + view.length + " buffer_is_ab=" + (view.buffer instanceof ArrayBuffer));

// Every geometry getter refuses a receiver that is not a typed array.
for (const key of ["buffer", "byteLength", "byteOffset", "length"]) {
  const g = getterOf(key);
  console.log("recv_" + key + "=" + t(function () { return g.call({}); }) + "/" + t(function () { return g.call([1, 2]); }) + "/" + t(function () { return g.call(new DataView(new ArrayBuffer(4))); }));
}
console.log("recv_null=" + t(function () { return getterOf("length").call(null); }));
console.log("recv_primitive=" + t(function () { return getterOf("length").call(5); }));
console.log("recv_cross_kind=" + t(function () { return getterOf("length").call(new Float64Array(2)); }));

// Symbol.toStringTag is the one that answers undefined instead of throwing —
// which is exactly why Object.prototype.toString says [object Object] for a
// non-typed-array receiver rather than raising.
const tagGetter = getterOf(Symbol.toStringTag);
console.log("tag_is_getter=" + (typeof tagGetter === "function"));
console.log("tag_plain=" + String(tagGetter.call({})));
console.log("tag_array=" + String(tagGetter.call([])));
console.log("tag_null=" + t(function () { return String(tagGetter.call(null)); }));
console.log("tag_typed=" + tagGetter.call(new Int16Array(1)) + "," + tagGetter.call(new Uint8ClampedArray(1)) + "," + tagGetter.call(new BigUint64Array(1)));
console.log("tag_desc=" + (function (): string {
  const d: any = Object.getOwnPropertyDescriptor(proto, Symbol.toStringTag);
  return "set:" + String(d.set) + " e:" + d.enumerable + " c:" + d.configurable;
})());
console.log("tostring_via_tag=" + Object.prototype.toString.call(new Float32Array(1)));

// DataView has its own three, and they refuse a typed array in turn.
const dvProto: any = DataView.prototype;
for (const key of ["buffer", "byteLength", "byteOffset"]) {
  const d: any = Object.getOwnPropertyDescriptor(dvProto, key);
  console.log("dv_" + key + "=get:" + typeof d.get + " on_typed:" + t(function () { return d.get.call(new Uint8Array(4)); }));
}
console.log("dv_values=" + (function (): string {
  const dv = new DataView(new ArrayBuffer(8), 2, 4);
  return dv.byteLength + "," + dv.byteOffset;
})());
console.log("dv_tag=" + Object.prototype.toString.call(new DataView(new ArrayBuffer(1))));

// After a detach the getters still answer — with zeroes — rather than throwing.
console.log("detached_geometry=" + t(function () {
  const buf = new ArrayBuffer(8);
  const v = new Uint8Array(buf, 2, 4);
  buf.transfer();
  return v.length + "," + v.byteLength + "," + v.byteOffset + "," + (v.buffer === buf) + "," + v.buffer.byteLength;
}));
console.log("detached_dv=" + t(function () {
  const buf = new ArrayBuffer(8);
  const dv = new DataView(buf);
  buf.transfer();
  return dv.byteLength;
}));

// The methods are on the shared intrinsic prototype, not on each kind.
console.log("methods_shared=" + ["slice", "subarray", "set", "fill", "sort", "indexOf", "join"].map(function (k) { return String((Uint8Array.prototype as any)[k] === (proto as any)[k]); }).join(","));
console.log("methods_not_own=" + ["slice", "subarray", "set"].map(function (k) { return String(Object.prototype.hasOwnProperty.call(Uint8Array.prototype, k)); }).join(","));
console.log("own_of_kind_proto=" + ["constructor", "BYTES_PER_ELEMENT"].map(function (k) { return k + ":" + Object.prototype.hasOwnProperty.call(Uint8Array.prototype, k); }).join(","));
console.log("bpe_on_proto=" + Uint8Array.prototype.BYTES_PER_ELEMENT + " on_intrinsic=" + String((proto as any).BYTES_PER_ELEMENT));
