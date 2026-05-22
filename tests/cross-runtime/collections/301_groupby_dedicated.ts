// Cross-runtime: Object.groupBy and Map.groupBy.
console.log("obj=" + JSON.stringify(Object.groupBy([1, 2, 3, 4], (x) => x % 2 ? "odd" : "even")));
const mg = Map.groupBy(["aa", "b", "cc"], (x) => x.length);
console.log("map=" + Array.from(mg.entries()).map(([k, v]) => k + ":" + v.join(",")).join("|"));
