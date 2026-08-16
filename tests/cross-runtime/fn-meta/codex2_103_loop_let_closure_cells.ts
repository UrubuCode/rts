// Cross-runtime: loop let bindings create a fresh closure cell per iteration.
const fns: Array<() => number> = [];
for (let i = 0; i < 4; i++) {
  fns.push(() => i);
}
console.log(fns.map((f) => f()).join(","));

