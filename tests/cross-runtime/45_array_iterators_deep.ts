// Cross-runtime compatibility: array iterators and entry ordering.
const arr = ["aa", "bb", "cc"];

const keys = Array.from(arr.keys()).join(",");
const values = Array.from(arr.values()).join(",");
const entries = Array.from(arr.entries())
  .map(([i, v]) => i + ":" + v)
  .join(",");

console.log("keys=" + keys);
console.log("values=" + values);
console.log("entries=" + entries);
console.log("findLast=" + [1, 2, 3, 2, 1].findLast((n) => n % 2 === 0));
console.log("findLastIndex=" + [1, 2, 3, 2, 1].findLastIndex((n) => n % 2 === 0));
