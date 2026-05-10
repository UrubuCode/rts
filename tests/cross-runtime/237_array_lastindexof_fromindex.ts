// Cross-runtime: Array.lastIndexOf with fromIndex
const arr = [1, 2, 3, 2, 1];
console.log("2=" + arr.lastIndexOf(2));
console.log("2_from_2=" + arr.lastIndexOf(2, 2));
console.log("2_from_1=" + arr.lastIndexOf(2, 1));

console.log("1=" + arr.lastIndexOf(1));
console.log("1_from_3=" + arr.lastIndexOf(1, 3));

// Negative fromIndex
console.log("2_from_-2=" + arr.lastIndexOf(2, -2));
console.log("1_from_-1=" + arr.lastIndexOf(1, -1));
