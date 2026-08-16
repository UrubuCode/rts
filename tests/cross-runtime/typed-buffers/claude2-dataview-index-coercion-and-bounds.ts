// Cross-runtime: how a DataView turns its byteOffset arguments into indices.
// Both the constructor and every accessor run ToIndex — a fraction truncates, a
// negative is a RangeError — and the bounds are checked against the VIEW window.

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
};

const buf = new ArrayBuffer(8);
new Uint8Array(buf).set([0, 1, 2, 3, 4, 5, 6, 7]);
const dv = new DataView(buf);

// Accessor offsets: ToIndex, applied before the bounds check.
console.log("plain=" + dv.getUint8(1));
console.log("fraction=" + t(function () { return dv.getUint8(1.9 as any); }));
console.log("string=" + t(function () { return dv.getUint8("2" as any); }));
console.log("bool=" + t(function () { return dv.getUint8(true as any); }));
console.log("null=" + t(function () { return dv.getUint8(null as any); }));
console.log("undefined=" + t(function () { return dv.getUint8(undefined as any); }));
console.log("nan=" + t(function () { return dv.getUint8(NaN as any); }));
console.log("negative=" + t(function () { return dv.getUint8(-1 as any); }));
console.log("negative_fraction=" + t(function () { return dv.getUint8(-0.5 as any); }));
console.log("infinity=" + t(function () { return dv.getUint8(Infinity as any); }));
console.log("bigint_offset=" + t(function () { return dv.getUint8(1n as any); }));
console.log("valueof=" + t(function () { return dv.getUint8({ valueOf: function () { return 3; } } as any); }));
console.log("no_argument=" + t(function () { return (dv as any).getUint8(); }));

// Bounds: the last byte of the read must still be inside the view.
console.log("last_byte=" + dv.getUint8(7) + " past=" + t(function () { return dv.getUint8(8); }));
console.log("u32_last=" + t(function () { return dv.getUint32(4).toString(16); }) + " u32_past=" + t(function () { return dv.getUint32(5); }));
console.log("f64_fits=" + t(function () { return String(dv.getFloat64(0) !== undefined); }) + " f64_past=" + t(function () { return dv.getFloat64(1); }));
console.log("set_past=" + t(function () { return dv.setUint32(5, 1); }));
console.log("set_negative=" + t(function () { return dv.setUint8(-1, 1); }));
console.log("set_returns=" + String(dv.setUint8(0, 9)) + " read_back=" + dv.getUint8(0));

// A windowed view is bounded by its own byteOffset/byteLength, not the buffer.
const win = new DataView(buf, 2, 4);
console.log("window=" + win.byteOffset + "," + win.byteLength + " buffer=" + win.buffer.byteLength);
console.log("window_zero=" + win.getUint8(0) + " window_last=" + win.getUint8(3));
console.log("window_past=" + t(function () { return win.getUint8(4); }));
console.log("window_u32=" + t(function () { return win.getUint32(0).toString(16); }) + " window_u32_past=" + t(function () { return win.getUint32(1); }));
console.log("window_writes_through=" + (function (): string {
  win.setUint8(0, 200);
  return new Uint8Array(buf)[2] + "";
})());

// Constructor arguments run the same coercion, with their own RangeErrors.
console.log("ctor_default_len=" + new DataView(new ArrayBuffer(4)).byteLength);
console.log("ctor_offset_only=" + new DataView(new ArrayBuffer(4), 1).byteLength);
console.log("ctor_offset_at_end=" + t(function () { return new DataView(new ArrayBuffer(4), 4).byteLength; }));
console.log("ctor_offset_past=" + t(function () { return new DataView(new ArrayBuffer(4), 5); }));
console.log("ctor_offset_negative=" + t(function () { return new DataView(new ArrayBuffer(4), -1); }));
console.log("ctor_offset_fraction=" + t(function () { return new DataView(new ArrayBuffer(4), 1.9).byteOffset; }));
console.log("ctor_len_past=" + t(function () { return new DataView(new ArrayBuffer(4), 2, 3); }));
console.log("ctor_len_negative=" + t(function () { return new DataView(new ArrayBuffer(4), 0, -1); }));
console.log("ctor_len_zero=" + t(function () { return new DataView(new ArrayBuffer(4), 2, 0).byteLength; }));
console.log("ctor_len_undefined=" + t(function () { return new DataView(new ArrayBuffer(4), 1, undefined).byteLength; }));
console.log("ctor_not_a_buffer=" + t(function () { return new DataView([1, 2, 3] as any); }));
console.log("ctor_typed_array=" + t(function () { return new DataView(new Uint8Array(4) as any); }));
console.log("ctor_no_args=" + t(function () { return new (DataView as any)(); }));
console.log("ctor_no_new=" + t(function () { return (DataView as any)(new ArrayBuffer(4)); }));
console.log("unaligned_read=" + t(function () {
  const b = new ArrayBuffer(8);
  const d = new DataView(b);
  d.setUint32(1, 0xdeadbeef);
  return d.getUint32(1).toString(16) + "/" + new Uint8Array(b)[1].toString(16);
}));
console.log("value_coercion=" + t(function () {
  const d = new DataView(new ArrayBuffer(4));
  d.setUint8(0, 300);
  d.setInt8(1, "129" as any);
  d.setUint8(2, NaN as any);
  d.setUint8(3, { valueOf: function () { return 7; } } as any);
  return d.getUint8(0) + "," + d.getInt8(1) + "," + d.getUint8(2) + "," + d.getUint8(3);
}));
