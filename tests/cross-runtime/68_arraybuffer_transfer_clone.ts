// Cross-runtime compatibility: structuredClone transfer of ArrayBuffer.
const buffer = new ArrayBuffer(4);
new Uint8Array(buffer).set([10, 20, 30, 40]);
const moved = structuredClone(buffer, { transfer: [buffer] });

console.log("orig_len=" + buffer.byteLength);
console.log("moved_len=" + moved.byteLength);
console.log("moved=" + Array.from(new Uint8Array(moved)).join(","));
