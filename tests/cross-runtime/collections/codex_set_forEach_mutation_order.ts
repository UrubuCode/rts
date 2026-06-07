// Cross-runtime: Set.forEach observes additions and skips deleted values.
const s = new Set([1, 2, 3]);
const seen: string[] = [];
s.forEach((value, same, set) => {
  seen.push(value + ":" + same + ":" + (set === s));
  if (value === 1) {
    s.delete(2);
    s.add(4);
  }
  if (value === 3) s.add(5);
});
console.log(seen.join("|"));
console.log([...s].join(","));
