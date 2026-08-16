// Cross-runtime: Map construction keeps the last duplicate value and first position.
const m = new Map<any, any>([["x", 1], ["y", 2], ["x", 3], [1, "n"], ["1", "s"]]);
console.log(JSON.stringify([...m]));
console.log(m.size, m.get("x"), m.get(1), m.get("1"));

