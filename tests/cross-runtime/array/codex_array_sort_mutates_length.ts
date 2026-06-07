// Cross-runtime: sort snapshots/uses elements while comparator mutates array.
const a: any[] = [3, 1, 2];
const log: string[] = [];
a.sort((x, y) => {
  log.push(x + ":" + y + ":len" + a.length);
  if (a.length === 3) {
    a.push(0);
    a[5] = 5;
  }
  return x - y;
});

console.log(a.length);
console.log(a.join(","));
console.log(log.length > 0);
