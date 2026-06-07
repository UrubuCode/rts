// Cross-runtime: Map iteration observes additions and skips deleted future keys.
const m = new Map<string, number>([["a", 1], ["b", 2], ["c", 3]]);
const seen: string[] = [];

for (const [k, v] of m) {
  seen.push(k + v);
  if (k === "a") {
    m.set("d", 4);
    m.delete("b");
  }
  if (k === "c") m.set("e", 5);
}

console.log(seen.join(","));
console.log([...m.keys()].join(","));
