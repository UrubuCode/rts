import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR3) Constructor chamado dentro de loop — cada iteracao
// produz instance distinta sem vazar this slot entre iteracoes.

function Item(value: number): void {
  this.v = value;
}

let total: number = 0;
for (let i = 0; i < 5; i++) {
  const it: any = new (Item as any)(i * 10);
  const v: number = it.v;
  total = total + v;
}
print("total=" + total);  // 0+10+20+30+40 = 100

// Nesting: ctor dentro de ctor
function Container(seedVal: number): void {
  const inner: any = new (Item as any)(seedVal * 2);
  const v: number = inner.v;
  this.child_v = v;
  this.seed = seedVal;
}

const c: any = new (Container as any)(7);
const seed: number = c.seed;
const childV: number = c.child_v;
print("c.seed=" + seed + " c.child_v=" + childV);

describe("new UserFn em loop e nested (#264 PR3)", () => {
  test("instances distintas no loop, this slot stack-based em nested", () =>
    expect(__rtsCapturedOutput).toBe(
      "total=100\n" +
      "c.seed=7 c.child_v=14\n"
    ));
});
