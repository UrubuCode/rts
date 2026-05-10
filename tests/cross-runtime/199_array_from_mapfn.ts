// Cross-runtime: Array.from with map function
const arr = Array.from([1, 2, 3], x => x * 2);
console.log("mapped=" + arr.join(","));

const str = Array.from("hello", c => c.toUpperCase());
console.log("upper=" + str.join(","));

const range = Array.from({ length: 5 }, (_, i) => i);
console.log("range=" + range.join(","));

const squares = Array.from({ length: 4 }, (_, i) => i * i);
console.log("squares=" + squares.join(","));

// From Set
const set = new Set([1, 2, 3]);
const doubled = Array.from(set, x => x * 2);
console.log("set=" + doubled.join(","));
