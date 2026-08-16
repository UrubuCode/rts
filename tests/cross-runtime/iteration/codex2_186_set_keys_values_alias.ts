// Cross-runtime: Set keys and values iterators yield the same sequence.
const s = new Set(["x", "y", "z"]);
console.log([...s.keys()].join(","));
console.log([...s.values()].join(","));
console.log([...s.entries()].map(([a, b]) => a + b).join(","));

