// Cross-runtime: Map iteration order via for-of.
const m = new Map([
  ["a", 1],
  ["b", 2],
  ["c", 3],
]);

let out = "";
for (const [k, v] of m) {
  if (k !== "b") out += k + ":" + v + ",";
}
console.log("map=" + out);
