// Cross-runtime: object operations that match all runtimes today.
const source = { a: 1, b: "x" };
const spread = { ...source, q: "tail" };
console.log("spread=" + JSON.stringify(spread));
console.log("own_a=" + spread.hasOwnProperty("a"));
console.log("own_z=" + spread.hasOwnProperty("z"));
