// Cross-runtime: for-in loop binding captured per iteration with let.
const obj: any = { a: 1, b: 2, c: 3 };
const fns: Function[] = [];
for (let k in obj) {
  fns.push(() => k + obj[k]);
}
console.log(fns.map(fn => fn()).join(","));

const gns: Function[] = [];
for (var q in obj) {
  gns.push(() => q);
}
console.log(gns.map(fn => fn()).join(","));
