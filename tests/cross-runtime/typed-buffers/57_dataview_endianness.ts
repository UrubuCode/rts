// Cross-runtime compatibility: ArrayBuffer/DataView endianness.
const buf = new ArrayBuffer(8);
const view = new DataView(buf);

view.setUint16(0, 0x1234, false);
view.setUint16(2, 0x5678, true);
view.setInt32(4, -42, false);

console.log("u16be=" + view.getUint16(0, false).toString(16));
console.log("u16le=" + view.getUint16(2, true).toString(16));
console.log("i32=" + view.getInt32(4, false));
console.log("bytes=" + Array.from(new Uint8Array(buf)).join(","));
