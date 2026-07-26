class P { x: number; y: number;
  constructor(x: number, y: number) { this.x = x; this.y = y; }
}
function run(n: number): number {
  let s = 0;
  for (let i = 0; i < n; i++) {
    const p = new P(i, i + 1);
    s = s + p.x * p.y;
  }
  return s;
}
const t0 = performance.now();
const r = run(3000000);
console.log(r, (performance.now() - t0).toFixed(2) + " ms");
