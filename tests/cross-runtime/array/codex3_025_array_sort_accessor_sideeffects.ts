// Cross-runtime: sort reads accessor values and writes the sorted result back.
const a: any[] = [3, 2, 1];
const seen: string[] = [];
Object.defineProperty(a, "0", {
  configurable: true,
  enumerable: true,
  get() { seen.push("get0"); return 3; },
  set(v) { seen.push("set0:" + v); },
});
a.sort((x, y) => x - y);
console.log(a[1], a[2]);
console.log(seen.join("|"));
