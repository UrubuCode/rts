// Cross-runtime: Object.values and entries iteration
const obj = { a: 1, b: 2, c: 3 };

const values = Object.values(obj);
console.log("values=" + values.join(","));

const entries = Object.entries(obj);
console.log("entries=" + entries.map(e => e.join(":")).join(","));

// With non-enumerable
Object.defineProperty(obj, "hidden", {
  value: 4,
  enumerable: false
});

const values2 = Object.values(obj);
console.log("values2=" + values2.join(","));

// Empty object
const empty = {};
console.log("empty_values=" + Object.values(empty).join(","));
console.log("empty_entries=" + Object.entries(empty).join(","));
