// Cross-runtime: JSON.stringify with replacer array
const obj = { a: 1, b: 2, c: 3, d: 4 };
const filtered = JSON.stringify(obj, ["a", "c"]);
console.log("filtered=" + filtered);

const all = JSON.stringify(obj, ["a", "b", "c", "d"]);
console.log("all=" + all);

const none = JSON.stringify(obj, []);
console.log("none=" + none);

// Nested
const nested = { x: { a: 1, b: 2 }, y: 3 };
const nestedFiltered = JSON.stringify(nested, ["x", "a"]);
console.log("nested=" + nestedFiltered);
