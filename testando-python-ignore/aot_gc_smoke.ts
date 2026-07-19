// AOT GC smoke — exercises gc string alloc/concat + classes + globals after the
// rts-macro removal (gc namespace + String/Number/Boolean classes hand-written).
class Pt {
  constructor(public x: number, public y: number) {}
  label(): string { return "(" + this.x + "," + this.y + ")"; }
}

function build(n: number): string {
  let acc = "";
  for (let i = 0; i < n; i++) {
    const p = new Pt(i, i * 2);
    acc = acc + p.label() + ";";
  }
  return acc;
}

const s = build(2000);
console.log("len=" + s.length);
console.log("isNaN=" + Number.isNaN(NaN));
console.log("bool=" + Boolean(0));
console.log("upper=" + "rts".toUpperCase());
console.log("json=" + JSON.stringify({ a: 1, b: [2, 3] }));
console.log("done");
