// Cross-runtime: for-of let bindings provide an independent closure per value.
const fns: Array<() => string> = [];
for (const value of ["a", "b", "c"]) {
  fns.push(() => value);
}
console.log(fns.map((f) => f()).join(","));
console.log(fns[0]() + fns[2]());

