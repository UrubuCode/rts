// Cross-runtime: Map and Set compose into a deterministic adjacency graph.
const graph = new Map<string, Set<string>>();
graph.set("a", new Set(["b", "c"]));
graph.set("b", new Set(["c"]));
graph.get("a")!.add("d");
console.log([...graph].map(([k, v]) => k + ">" + [...v].join("")).join("|"));
console.log(graph.get("a")!.has("c"), graph.get("b")!.size);

