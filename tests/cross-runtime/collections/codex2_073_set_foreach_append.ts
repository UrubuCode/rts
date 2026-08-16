// Cross-runtime: Set.forEach visits new values appended before completion.
const s = new Set([1, 2]);
const seen: number[] = [];
s.forEach((v) => {
  seen.push(v);
  if (v === 1) s.add(3);
});
console.log(seen.join(","), [...s].join(","));

