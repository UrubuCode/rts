// Cross-runtime: nested for-of loops maintain independent iterator state.
const out: string[] = [];
for (const a of ["x", "y"]) {
  for (const b of [1, 2, 3]) {
    out.push(a + b);
  }
}
console.log(out.join(","));

