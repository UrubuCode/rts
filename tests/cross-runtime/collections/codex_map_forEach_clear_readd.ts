// Cross-runtime: Map.forEach with clear and re-add during traversal.
const m = new Map<any, any>([["a", 1], ["b", 2], ["c", 3]]);
const seen: string[] = [];
m.forEach((v, k) => {
  seen.push(k + v);
  if (k === "a") {
    m.clear();
    m.set("d", 4);
    m.set("e", 5);
  }
});
console.log(seen.join(","));
console.log([...m.entries()].map(([k, v]) => k + v).join(","));
