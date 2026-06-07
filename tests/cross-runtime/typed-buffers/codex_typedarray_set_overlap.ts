// Cross-runtime: TypedArray.set handles overlapping source ranges.
const a = new Uint8Array([1, 2, 3, 4, 5, 6]);
a.set(a.subarray(0, 4), 2);
console.log(Array.from(a).join(","));

const b = new Int8Array(4);
b.set([127, 128, -129, 260]);
console.log(Array.from(b).join(","));
