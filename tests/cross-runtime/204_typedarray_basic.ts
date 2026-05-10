// Cross-runtime: TypedArray basics
const arr8 = new Uint8Array([1, 2, 3, 4]);
console.log("length=" + arr8.length);
console.log("0=" + arr8[0]);
console.log("3=" + arr8[3]);

arr8[1] = 10;
console.log("modified=" + arr8[1]);

const arr16 = new Uint16Array(3);
arr16[0] = 100;
arr16[1] = 200;
arr16[2] = 300;
console.log("16bit=" + Array.from(arr16).join(","));

const arr32 = new Int32Array([1, -2, 3]);
console.log("32bit=" + Array.from(arr32).join(","));
