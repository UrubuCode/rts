// Cross-runtime: Map.forEach observes entries appended during iteration.
const m = new Map([["a", 1], ["b", 2]]);
const seen: string[] = [];
m.forEach((v, k) => {
  seen.push(k + v);
  if (k === "a") m.set("c", 3);
});
console.log(seen.join("|"));
console.log([...m.keys()].join(","));

