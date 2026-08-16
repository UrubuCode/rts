// Cross-runtime: updating a Map value does not move its key.
const m = new Map([["a", 1], ["b", 2], ["c", 3]]);
m.set("b", 20);
m.set("a", 10);
console.log([...m.keys()].join(","));
console.log([...m.values()].join(","));

