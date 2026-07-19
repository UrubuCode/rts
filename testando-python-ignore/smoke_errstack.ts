class Box { v: number; constructor(v: number) { this.v = v; } }
const b = new Box(3.5);
try {
  throw new Error("boom");
} catch (e: any) {
  const s = e.stack;
  console.log(s.length > 0 ? "stack-ok" : "stack-empty");
}
console.log((1e21).toString());
console.log(b.v.toString());
