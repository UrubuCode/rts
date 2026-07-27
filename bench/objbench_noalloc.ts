// V2 — the SAME field arithmetic as objbench.ts, with the allocation moved OUT
// of the loop. objbench.ts constructs 3M objects; this constructs ONE and reads
// its fields 3M times.
//
// The pair is a falsification test for "allocation dominates the object path".
// If it does, V2 must be dramatically faster than V1 here while a runtime that
// does not allocate per iteration (Bun) shows a much smaller spread.
class P { x: number; y: number;
  constructor(x: number, y: number) { this.x = x; this.y = y; }
}
function run(n: number): number {
  let s = 0;
  const p = new P(1, 2);
  for (let i = 0; i < n; i++) {
    s = s + p.x * p.y;
  }
  return s;
}
const t0 = performance.now();
const r = run(3000000);
console.log(r, (performance.now() - t0).toFixed(2) + " ms");
