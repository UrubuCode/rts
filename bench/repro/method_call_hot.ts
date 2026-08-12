// One line of bench/analytic.ts, alone: a depth-one method call, which is the
// path the remembered-refusal sign check is paid on.
class Callee {
  v: number = 1;
  m(x: number): number { return x + this.v; }
}
const callee = new Callee();
function run(n: number): number {
  let a = 0;
  for (let i = 0; i < n; i++) a = callee.m(a);
  return a;
}
run(1000000);
const t0 = performance.now();
const r = run(20000000);
const ns = (performance.now() - t0) * 1e6 / 20000000;
console.log(r, ns.toFixed(3) + " ns/op");
