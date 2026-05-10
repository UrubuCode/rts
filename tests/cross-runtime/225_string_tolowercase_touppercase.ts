// Cross-runtime: String case conversion
const str = "Hello World";
console.log("lower=" + str.toLowerCase());
console.log("upper=" + str.toUpperCase());

const lower = "hello";
console.log("already_lower=" + lower.toLowerCase());

const upper = "HELLO";
console.log("already_upper=" + upper.toUpperCase());

const mixed = "HeLLo WoRLd";
console.log("mixed_lower=" + mixed.toLowerCase());
console.log("mixed_upper=" + mixed.toUpperCase());

const numbers = "abc123";
console.log("with_nums=" + numbers.toUpperCase());
