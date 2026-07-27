// Isolates arithmetic on `: number` class FIELDS: one construction, then a hot
// loop that only reads fields and computes. Complements objbench.ts, whose loop
// is dominated by allocation + VEC traffic and so masks this axis.
class V { x: number; y: number; z: number;
  constructor(x: number, y: number, z: number) { this.x = x; this.y = y; this.z = z; }
}
function run(v: V, n: number): number {
  let s = 0;
  for (let i = 0; i < n; i++) {
    s = s + v.x * v.y + v.z * v.x - v.y * v.z + v.x / v.y;
  }
  return s;
}
const v = new V(1.5, 2.5, 3.5);
const t0 = performance.now();
const r = run(v, 5000000);
console.log(r.toFixed(2), (performance.now() - t0).toFixed(2) + " ms");
