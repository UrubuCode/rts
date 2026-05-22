// Cross-runtime: array search helpers with negative fromIndex.
const arr = [1, 2, 3, 2, 1, 2];
console.log("indexOf=" + arr.indexOf(2, -3));
console.log("lastIndexOf=" + arr.lastIndexOf(2, -2));
console.log("includes=" + arr.includes(2, -2));
