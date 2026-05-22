// Cross-runtime: String.search returns index
const str = "hello world";
console.log("world=" + str.search(/world/));
console.log("hello=" + str.search(/hello/));
console.log("not_found=" + str.search(/xyz/));

console.log("digit=" + "abc123def".search(/\d/));
console.log("letter=" + "123abc".search(/[a-z]/));

// Case insensitive
console.log("case=" + "Hello".search(/hello/i));
