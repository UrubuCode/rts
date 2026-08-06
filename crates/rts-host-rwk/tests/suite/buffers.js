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

// `Uint8ClampedArray` SATURATES where every other kind wraps, and rounds a
// half to EVEN where they truncate. Both are the specification, and both are
// invisible in a test written from small whole numbers.
let clamped = new Uint8ClampedArray(1);
clamped[0] = 300;
check("clamp-high", clamped[0] === 255);
clamped[0] = -5;
check("clamp-low", clamped[0] === 0);
clamped[0] = 0.5;
check("clamp-half-to-even-down", clamped[0] === 0);
clamped[0] = 1.5;
check("clamp-half-to-even-up", clamped[0] === 2);
check("clamp-bytes-per-element", Uint8ClampedArray.BYTES_PER_ELEMENT === 1);
// And an ordinary `Uint8Array` still wraps, which is the pair.
let wrapping = new Uint8Array(1);
wrapping[0] = 300;
check("wrap-still-wraps", wrapping[0] === 44);

// The two classes whose elements are BIGINTS. Sixty-four bits is exactly the
// width where a double stops carrying an integer element, which is why these are
// a different element type rather than a wider row.
let big = new BigInt64Array(2);
check("big-length", big.length === 2);
check("big-byte-length", big.byteLength === 16);
check("big-bytes-per-element", BigInt64Array.BYTES_PER_ELEMENT === 8);
check("big-zeroed", big[0] === 0n);
check("big-reads-a-bigint", typeof big[0] === "bigint");

big[0] = 7n;
check("big-round-trip", big[0] === 7n);
big[0] = -1n;
check("big-signed", big[0] === -1n);

// Past 2^53, where a double could not have told two values apart. This is the
// whole reason the class exists, and the check that fails if the element ever
// travels through one.
big[0] = 9007199254740993n;
check("big-past-double", big[0] === 9007199254740993n);
big[0] = 9223372036854775807n;
check("big-max-signed", big[0] === 9223372036854775807n);
// A store into a fixed width wraps, exactly as `Int8Array` given 256 stores 0.
big[0] = 9223372036854775808n;
check("big-wraps", big[0] === -9223372036854775808n);

let unsignedWide = new BigUint64Array(1);
unsignedWide[0] = 18446744073709551615n;
check("unsigned-max", unsignedWide[0] === 18446744073709551615n);
unsignedWide[0] = -1n;
check("unsigned-wraps-negative", unsignedWide[0] === 18446744073709551615n);
check("unsigned-never-negative", unsignedWide[0] > 0n);

// The two read the same bytes and differ only in whether the top bit is a sign.
let shared64 = new ArrayBuffer(8);
let asSigned = new BigInt64Array(shared64);
let asUnsigned = new BigUint64Array(shared64);
asSigned[0] = -1n;
check("same-bytes-two-signs", asUnsigned[0] === 18446744073709551615n);

// A NUMBER written into a bigint element is a `TypeError` in the language.
// This cannot throw, so the write is dropped — the element keeps what it held,
// where coercing would make a program no other engine accepts answer something.
big[0] = 5n;
big[0] = 5;
check("refuses-a-number", big[0] === 5n);
// And the other direction, on an ordinary typed array.
let plain = new Uint8Array(1);
plain[0] = 3;
plain[0] = 9n;
check("numeric-refuses-a-bigint", plain[0] === 3);

check("big-at", (function () {
    let t = new BigInt64Array(2);
    t[1] = 4n;
    return t.at(-1) === 4n;
})());
check("big-fill", (function () {
    let t = new BigInt64Array(2);
    t.fill(6n);
    return t[0] === 6n && t[1] === 6n;
})());
check("big-from-array", (function () {
    let t = new BigInt64Array([1n, 2n, 3n]);
    return t.length === 3 && t[2] === 3n;
})());
check("big-slice", (function () {
    let t = new BigInt64Array([1n, 2n, 3n]);
    return t.slice(1).length === 2 && t.slice(1)[0] === 2n;
})());
check("big-set", (function () {
    let t = new BigInt64Array(3);
    t.set(new BigInt64Array([8n, 9n]), 1);
    return t[1] === 8n && t[2] === 9n && t[0] === 0n;
})());
// A copy between the two families copies nothing, for the same reason a single
// write is dropped.
check("no-cross-family-copy", (function () {
    let t = new BigInt64Array(1);
    t[0] = 3n;
    t.set(new Uint8Array([9]));
    return t[0] === 3n;
})());

return failed;
