// Cross-runtime: String.slice with negative indices
const str = "hello world";
console.log("slice_-5=" + str.slice(-5));
console.log("slice_-5_-2=" + str.slice(-5, -2));
console.log("slice_0_-6=" + str.slice(0, -6));
console.log("slice_-11=" + str.slice(-11));

console.log("slice_6_-1=" + str.slice(6, -1));
console.log("slice_-100=" + str.slice(-100));
console.log("slice_5_2=" + str.slice(5, 2));
