// Cross-runtime: subarray clamps negative bounds and shares storage with the source.
const source = new Uint16Array([10, 20, 30, 40, 50]);
const middle = source.subarray(-4, -1);
console.log(middle.join(","), middle.byteOffset, middle.byteLength);
middle[1] = 99;
console.log(source.join(","));
console.log(middle.buffer === source.buffer);

