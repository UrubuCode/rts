// Cross-runtime: map snapshots length but observes getters and skips deleted future indexes.
const a: any[] = [1, 2, 3, 4];
Object.defineProperty(a, "0", {
  configurable: true,
  get() { delete a[2]; a.push(5); return 10; },
});
const seen: string[] = [];
const mapped = a.map((v, i) => { seen.push(i + ":" + v); return v * 2; });
console.log(seen.join("|"));
console.log(mapped.length, Object.keys(mapped).join(","), JSON.stringify(mapped));
console.log(a.length);

