// Cross-runtime compatibility: typed array methods.
const arr = new Uint8Array([10, 20, 30, 40, 50]);
const sub = arr.subarray(1, 4);
console.log("sub=" + Array.from(sub).join(","));

const slice = arr.slice(2);
console.log("slice=" + Array.from(slice).join(","));

arr.set([99, 88], 1);
console.log("set=" + Array.from(arr).join(","));

arr.copyWithin(0, 3, 5);
console.log("copyWithin=" + Array.from(arr).join(","));

arr.fill(7, 1, 3);
console.log("fill=" + Array.from(arr).join(","));
console.log("includes=" + arr.includes(7));
console.log("at=" + arr.at(-1));
