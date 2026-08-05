// `ArrayBuffer`, `DataView`, and the typed arrays.
let failed = "";
function check(name, held) { if (!held) { failed = failed + name + ","; } }

let buffer = new ArrayBuffer(8);
check("byte-length", buffer.byteLength === 8);
check("is-view-false", ArrayBuffer.isView(buffer) === false);
check("slice", buffer.slice(0, 4).byteLength === 4);

let bytes = new Uint8Array(4);
check("typed-length", bytes.length === 4);
check("typed-byte-length", bytes.byteLength === 4);
check("typed-zeroed", bytes[0] === 0);
check("is-view-true", ArrayBuffer.isView(bytes));
check("bytes-per-element", Uint8Array.BYTES_PER_ELEMENT === 1);
check("bytes-per-element-wide", Float64Array.BYTES_PER_ELEMENT === 8);

bytes[0] = 7;
check("indexed-write", bytes[0] === 7);
check("indexed-past-end", bytes[9] === undefined);
// A typed array does not grow: a write past the end stores nothing anybody can
// read back, rather than falling through to a property.
bytes[9] = 1;
check("no-growth", bytes[9] === undefined && bytes.length === 4);
check("at-negative", bytes.at(-4) === 7);

// Integer conversion on write is modular WRAP, not saturation. Getting that
// wrong is a wrong answer that looks right for every value under 128.
let signed = new Int8Array(1);
signed[0] = 300;
check("wrap-int8", signed[0] === 44);
signed[0] = 128;
check("wrap-int8-boundary", signed[0] === -128);
let unsigned = new Uint8Array(1);
unsigned[0] = -1;
check("wrap-uint8", unsigned[0] === 255);

check("from-array", new Uint8Array([1, 2, 3]).length === 3);
check("from-array-values", new Uint8Array([1, 2, 3])[1] === 2);
check("fill", new Uint8Array(2).fill(9)[1] === 9);
check("subarray", new Uint8Array([1, 2, 3]).subarray(1).length === 2);
check("slice-typed", new Uint8Array([1, 2, 3]).slice(1).length === 2);

// Two views over one buffer see each other's writes. That is the aliasing
// contract, and a copy would satisfy every test that used one view at a time.
let shared = new ArrayBuffer(4);
let asBytes = new Uint8Array(shared);
let asWords = new Uint32Array(shared);
asWords[0] = 0;
asBytes[0] = 1;
check("aliasing", asWords[0] !== 0);
check("same-buffer", asBytes.byteLength === 4 && asWords.length === 1);

let offset = new Uint8Array(shared, 1, 2);
check("offset-length", offset.length === 2);
check("offset-byte-offset", offset.byteOffset === 1);
check("offset-aliases", (function () {
    asBytes[1] = 5;
    return offset[0] === 5;
})());

let view = new DataView(new ArrayBuffer(8));
check("view-byte-length", view.byteLength === 8);
check("view-byte-offset", view.byteOffset === 0);

view.setUint8(0, 200);
check("get-uint8", view.getUint8(0) === 200);
check("get-int8", view.getInt8(0) === -56);

// The default is BIG-endian, which surprises people and is the specification's.
view.setUint16(0, 1);
check("big-endian-default", view.getUint8(0) === 0 && view.getUint8(1) === 1);
view.setUint16(0, 1, true);
check("little-endian-flag", view.getUint8(0) === 1 && view.getUint8(1) === 0);
check("round-trip-uint16", (function () { view.setUint16(2, 65535); return view.getUint16(2) === 65535; })());
check("round-trip-int32", (function () { view.setInt32(0, -7); return view.getInt32(0) === -7; })());
check("round-trip-float64", (function () { view.setFloat64(0, 1.5); return view.getFloat64(0) === 1.5; })());
check("round-trip-float32", (function () { view.setFloat32(0, 1.5); return view.getFloat32(0) === 1.5; })());

check("float-array", (function () {
    let f = new Float64Array(2);
    f[0] = 1.5;
    return f[0] === 1.5 && f.byteLength === 16;
})());

return failed;
