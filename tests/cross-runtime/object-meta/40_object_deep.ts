// Cross-runtime future coverage: fuller Object APIs.
const source = { a: 1, b: "x" };
const extra = { c: "y", d: "z" };
const merged = Object.assign({}, source, extra);

console.log("keys=" + Object.keys(merged).join(","));
console.log("values=" + Object.values(merged).join(","));
console.log("entries=" + Object.entries(merged).map((pair) => pair[0] + "=" + pair[1]).join(","));

const rebuilt = Object.fromEntries([
  ["x", "10"],
  ["y", "20"],
]);
console.log("fromEntries=" + rebuilt.x + ":" + rebuilt.y);

const frozen = Object.freeze({ p: "ok" });
console.log("frozen=" + Object.isFrozen(frozen));

const spread = { ...source, q: "tail" };
console.log("spread=" + JSON.stringify(spread));
console.log("in_a=" + ("a" in spread));
console.log("in_z=" + ("z" in spread));
console.log("own_a=" + Object.prototype.hasOwnProperty.call(spread, "a"));
console.log("own_z=" + Object.prototype.hasOwnProperty.call(spread, "z"));
