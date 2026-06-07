// Cross-runtime: Set iteration with delete/add during traversal.
const s = new Set<number>([1, 2, 3]);
const seen: number[] = [];

for (const v of s) {
  seen.push(v);
  if (v === 1) {
    s.delete(2);
    s.add(4);
  }
  if (v === 3) s.add(5);
}

console.log(seen.join(","));
console.log([...s].join(","));
