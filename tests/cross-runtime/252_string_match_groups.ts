// Cross-runtime: String.match returns array with index
const str = "hello world";
const match = str.match(/world/);

console.log("found=" + (match !== null));
console.log("value=" + (match ? match[0] : "null"));
console.log("index=" + (match ? match.index : "null"));
console.log("input=" + (match ? match.input : "null"));

const noMatch = str.match(/xyz/);
console.log("noMatch=" + noMatch);
