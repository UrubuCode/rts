// Cross-runtime: DataView offset and endian interactions.
const buf = new ArrayBuffer(12);
const dv = new DataView(buf, 2, 8);
dv.setUint16(0, 0x1234, false);
dv.setUint16(2, 0x5678, true);
dv.setInt32(4, -2, true);

const bytes = Array.from(new Uint8Array(buf)).map(x => x.toString(16).padStart(2, "0")).join(" ");
console.log(bytes);
console.log(dv.getUint16(0, false).toString(16));
console.log(dv.getUint16(2, true).toString(16));
console.log(dv.getInt32(4, true));
