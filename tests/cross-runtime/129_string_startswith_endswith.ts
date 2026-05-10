// Cross-runtime: String startsWith and endsWith
const str = "hello world";
console.log("starts_hello=" + str.startsWith("hello"));
console.log("starts_world=" + str.startsWith("world"));
console.log("starts_world_6=" + str.startsWith("world", 6));
console.log("ends_world=" + str.endsWith("world"));
console.log("ends_hello=" + str.endsWith("hello"));
console.log("ends_hello_5=" + str.endsWith("hello", 5));

const empty = "";
console.log("empty_starts=" + empty.startsWith(""));
console.log("empty_ends=" + empty.endsWith(""));
