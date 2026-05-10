// Cross-runtime: String.includes with position
const str = "hello world";
console.log("world=" + str.includes("world"));
console.log("world_0=" + str.includes("world", 0));
console.log("world_6=" + str.includes("world", 6));
console.log("world_7=" + str.includes("world", 7));

console.log("hello_6=" + str.includes("hello", 6));
console.log("o=" + str.includes("o"));
console.log("o_5=" + str.includes("o", 5));
console.log("o_8=" + str.includes("o", 8));
