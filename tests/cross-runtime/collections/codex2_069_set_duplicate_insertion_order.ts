// Cross-runtime: Set removes duplicates without disturbing first insertion order.
const s = new Set<any>(["a", "b", "a", "c", "b"]);
console.log([...s].join(","));
s.add("a");
console.log([...s].join(","), s.size);

