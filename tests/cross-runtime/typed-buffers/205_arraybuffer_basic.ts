// Cross-runtime: ArrayBuffer basics
const buffer = new ArrayBuffer(8);
console.log("byteLength=" + buffer.byteLength);

const view8 = new Uint8Array(buffer);
view8[0] = 255;
view8[1] = 128;
console.log("view8_0=" + view8[0]);
console.log("view8_1=" + view8[1]);

const view32 = new Uint32Array(buffer);
console.log("view32_0=" + view32[0]);

// Slice
const sliced = buffer.slice(0, 4);
console.log("sliced=" + sliced.byteLength);
