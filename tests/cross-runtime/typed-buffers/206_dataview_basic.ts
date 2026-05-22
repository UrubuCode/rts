// Cross-runtime: DataView basics
const buffer = new ArrayBuffer(8);
const view = new DataView(buffer);

view.setUint8(0, 255);
view.setUint8(1, 128);
console.log("get0=" + view.getUint8(0));
console.log("get1=" + view.getUint8(1));

view.setUint16(2, 1000);
console.log("get16=" + view.getUint16(2));

view.setInt32(4, -123456);
console.log("get32=" + view.getInt32(4));

console.log("byteLength=" + view.byteLength);
console.log("byteOffset=" + view.byteOffset);
