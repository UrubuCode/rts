// Cross-runtime: structuredClone with ArrayBuffer transfer detaches source.
const buf = new ArrayBuffer(4);
new Uint8Array(buf).set([1, 2, 3, 4]);
const cloned = structuredClone({ buf }, { transfer: [buf] });
console.log(buf.byteLength);
console.log(cloned.buf.byteLength);
console.log(Array.from(new Uint8Array(cloned.buf)).join(","));
