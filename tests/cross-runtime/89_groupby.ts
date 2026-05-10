// Cross-runtime compatibility: Object.groupBy and Map.groupBy.
const objGroup = Object.groupBy([1, 2, 3, 4], (x) => (x % 2 ? "odd" : "even"));
console.log("obj=" + JSON.stringify(objGroup));

const mapGroup = Map.groupBy([1, 2, 3], (x) => x % 2);
console.log(
  "map=" + Array.from(mapGroup.entries())
    .map(([k, v]) => k + ":" + v.join(","))
    .join("|"),
);
