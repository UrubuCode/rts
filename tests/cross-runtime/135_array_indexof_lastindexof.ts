// Cross-runtime: Array indexOf and lastIndexOf
const arr = [1, 2, 3, 2, 1];
console.log("indexOf_2=" + arr.indexOf(2));
console.log("lastIndexOf_2=" + arr.lastIndexOf(2));
console.log("indexOf_5=" + arr.indexOf(5));
console.log("lastIndexOf_5=" + arr.lastIndexOf(5));

console.log("indexOf_from=" + arr.indexOf(2, 2));
console.log("lastIndexOf_from=" + arr.lastIndexOf(2, 2));

// NaN behavior
const withNaN = [1, NaN, 3];
console.log("indexOf_NaN=" + withNaN.indexOf(NaN));

// String
const str = "hello world hello";
console.log("str_indexOf=" + str.indexOf("hello"));
console.log("str_lastIndexOf=" + str.lastIndexOf("hello"));
console.log("str_indexOf_from=" + str.indexOf("hello", 5));
