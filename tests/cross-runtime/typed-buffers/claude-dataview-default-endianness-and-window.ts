// Cross-runtime: DataView defaults to BIG endian — the opposite of a typed
// array, which always follows the machine — and every access is bounds-checked
// against the view's own window, not the whole buffer.

const dv = new DataView(new ArrayBuffer(8));

// The littleEndian argument is optional and defaults to false.
dv.setUint16(0, 0x1234);
console.log("be_bytes=" + new Uint8Array(dv.buffer)[0] + "," + new Uint8Array(dv.buffer)[1]);
console.log("be_read=" + dv.getUint16(0).toString(16));
console.log("le_read_of_be_write=" + dv.getUint16(0, true).toString(16));
console.log("default_is_big=" + (dv.getUint16(0) === dv.getUint16(0, false)));
console.log("undefined_arg_is_big=" + (dv.getUint16(0, undefined) === dv.getUint16(0, false)));
console.log("truthy_arg=" + (dv.getUint16(0, 1 as any) === dv.getUint16(0, true)));

dv.setUint32(0, 0x01020304, true);
console.log("le_bytes=" + Array.from(new Uint8Array(dv.buffer, 0, 4)).join(","));
console.log("le_back=" + dv.getUint32(0, true).toString(16) + " be=" + dv.getUint32(0).toString(16));

// Unaligned offsets are legal for a DataView.
dv.setFloat64(0, 0);
dv.setInt32(1, -2, true);
console.log("unaligned_le=" + dv.getInt32(1, true));
console.log("unaligned_be=" + dv.getInt32(1));
dv.setUint16(3, 0xabcd);
console.log("unaligned_u16=" + dv.getUint16(3).toString(16) + " swapped=" + dv.getUint16(3, true).toString(16));

// Every width, written and read back both ways.
const one = new DataView(new ArrayBuffer(8));
one.setInt8(0, -1);
console.log("i8=" + one.getInt8(0) + " u8=" + one.getUint8(0));
one.setInt16(0, -2, true);
console.log("i16=" + one.getInt16(0, true) + " u16=" + one.getUint16(0, true));
one.setFloat32(0, 1.5, true);
console.log("f32=" + one.getFloat32(0, true) + " swapped=" + one.getFloat32(0));
one.setFloat64(0, -0);
console.log("f64_negzero=" + Object.is(one.getFloat64(0), -0));
one.setBigUint64(0, 2n ** 64n - 1n);
console.log("bigu=" + one.getBigUint64(0) + " bigi=" + one.getBigInt64(0));
one.setBigInt64(0, -1n, true);
console.log("bigi_le=" + one.getBigInt64(0, true));

// A view over a window: byteOffset shifts index 0, byteLength ends it.
const buf = new ArrayBuffer(10);
new Uint8Array(buf).set([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
const win = new DataView(buf, 3, 4);
console.log("win=" + win.byteOffset + "," + win.byteLength + " buflen=" + win.buffer.byteLength);
console.log("win_read0=" + win.getUint8(0) + " read3=" + win.getUint8(3));
win.setUint8(0, 99);
console.log("win_write_lands_at=" + new Uint8Array(buf)[3]);
try {
  win.getUint8(4);
  console.log("win_past=no-throw");
} catch (e: any) {
  console.log("win_past=" + e.constructor.name);
}
try {
  win.getUint32(1);
  console.log("win_straddle=" + "no-throw");
} catch (e: any) {
  console.log("win_straddle=" + e.constructor.name);
}
console.log("win_fits=" + win.getUint32(0).toString(16));

// A view with no length argument runs to the end of the buffer.
const tail = new DataView(buf, 8);
console.log("tail=" + tail.byteOffset + "," + tail.byteLength);

// Bounds and constructor errors.
try {
  dv.getUint32(5);
  console.log("read_past=no-throw");
} catch (e: any) {
  console.log("read_past=" + e.constructor.name);
}
try {
  dv.getUint8(-1);
  console.log("read_negative=no-throw");
} catch (e: any) {
  console.log("read_negative=" + e.constructor.name);
}
try {
  new DataView(new ArrayBuffer(4), 5);
  console.log("ctor_offset=no-throw");
} catch (e: any) {
  console.log("ctor_offset=" + e.constructor.name);
}
try {
  new DataView(new ArrayBuffer(4), 2, 3);
  console.log("ctor_length=no-throw");
} catch (e: any) {
  console.log("ctor_length=" + e.constructor.name);
}
try {
  new DataView({} as any);
  console.log("ctor_notbuffer=no-throw");
} catch (e: any) {
  console.log("ctor_notbuffer=" + e.constructor.name);
}
console.log("ctor_at_end=" + new DataView(new ArrayBuffer(4), 4).byteLength);

// A fractional index truncates; undefined and NaN read index 0.
const frac = new DataView(new ArrayBuffer(4));
frac.setUint8(1, 55);
console.log("frac_index=" + frac.getUint8(1.9));
frac.setUint8(0, 7);
console.log("nan_index=" + frac.getUint8(NaN as any) + "," + frac.getUint8(undefined as any));

// The tag and the prototype chain.
console.log("tag=" + Object.prototype.toString.call(dv));
console.log("proto=" + (Object.getPrototypeOf(dv) === DataView.prototype));
console.log("proto_of_ctor=" + (Object.getPrototypeOf(DataView) === Function.prototype));
