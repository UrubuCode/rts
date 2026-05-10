// Cross-runtime: Array.isArray
console.log("array=" + Array.isArray([1, 2, 3]));
console.log("empty=" + Array.isArray([]));
console.log("object=" + Array.isArray({ length: 0 }));
console.log("string=" + Array.isArray("hello"));
console.log("number=" + Array.isArray(123));
console.log("null=" + Array.isArray(null));
console.log("undefined=" + Array.isArray(undefined));

// Array-like
const arrayLike = { 0: "a", 1: "b", length: 2 };
console.log("arraylike=" + Array.isArray(arrayLike));

// Arguments would be array-like but we can't test it in this context
