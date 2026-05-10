// Cross-runtime: Array.indexOf with fromIndex
const arr = [1, 2, 3, 2, 1];
console.log("2=" + arr.indexOf(2));
console.log("2_from_2=" + arr.indexOf(2, 2));
console.log("2_from_3=" + arr.indexOf(2, 3));

console.log("1=" + arr.indexOf(1));
console.log("1_from_1=" + arr.indexOf(1, 1));

// Negative fromIndex
console.log("2_from_-2=" + arr.indexOf(2, -2));
console.log("1_from_-1=" + arr.indexOf(1, -1));
