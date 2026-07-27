// Companion to objbench.ts: same loop, but the class carries METHODS. The
// per-class prototype publish costs one extern call PER METHOD PER `new`, so
// this is where hoisting the wiring out of the loop shows up (objbench.ts's
// class has none, and only sheds the single class_proto_init).
class V {
  x: number; y: number;
  constructor(x: number, y: number) { this.x = x; this.y = y; }
  dot(o: V): number { return this.x * o.x + this.y * o.y; }
  norm2(): number { return this.x * this.x + this.y * this.y; }
  scaled(k: number): number { return this.x * k + this.y * k; }
  sum(): number { return this.x + this.y; }
}
function run(n: number): number {
  let s = 0;
  for (let i = 0; i < n; i++) {
    const v = new V(i, i + 1);
    s = s + v.norm2();
  }
  return s;
}
const t0 = performance.now();
const r = run(1000000);
console.log(r, (performance.now() - t0).toFixed(2) + " ms");
