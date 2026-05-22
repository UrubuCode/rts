// Cross-runtime: Set basic operations.
const s = new Set([1, 2, 3, 2, 1]);
console.log(s.size);  // 3 — deduped
console.log(s.has(2));
console.log(s.has(9));

s.add(4);
s.add(2);  // duplicate, no-op
console.log(s.size);  // 4

s.delete(1);
console.log(s.size);  // 3
console.log(s.has(1));

const vals: number[] = [];
s.forEach(v => vals.push(v));
console.log(vals.join(","));

s.clear();
console.log(s.size);
