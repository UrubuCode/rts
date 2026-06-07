// Cross-runtime: reduce observes holes filled before visit and skips deleted indexes.
const a = [1, , 3, 4] as any[];
const seen: string[] = [];
const sum = a.reduce((acc, v, i) => {
  seen.push(i + ":" + v);
  if (i === 0) a[1] = 2;
  if (i === 1) delete a[3];
  return acc + v;
}, 0);

console.log("sum=" + sum);
console.log("seen=" + seen.join(","));
console.log("keys=" + Object.keys(a).join(","));
