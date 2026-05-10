// Cross-runtime: String.lastIndexOf with fromIndex
const str = "hello world hello";
console.log("basic=" + str.lastIndexOf("hello"));
console.log("from_10=" + str.lastIndexOf("hello", 10));
console.log("from_5=" + str.lastIndexOf("hello", 5));
console.log("from_0=" + str.lastIndexOf("hello", 0));

console.log("o=" + str.lastIndexOf("o"));
console.log("o_from_10=" + str.lastIndexOf("o", 10));
console.log("o_from_5=" + str.lastIndexOf("o", 5));
