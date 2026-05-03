import { describe, test, expect } from "rts:test";
import { collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR3) Constructor chamado dentro de loop — cada iteracao
// produz instance distinta sem vazar this slot entre iteracoes.

function Item(value: number): void {
  collections.map_set(this as any, "v", value);
}

let total: number = 0;
for (let i = 0; i < 5; i++) {
  const it: number = new (Item as any)(i * 10);
  const v: number = collections.map_get(it, "v");
  total = total + v;
}
print("total=" + total);  // 0+10+20+30+40 = 100

// Nesting: ctor dentro de ctor
function Container(seedVal: number): void {
  const inner: number = new (Item as any)(seedVal * 2);
  const v: number = collections.map_get(inner, "v");
  collections.map_set(this as any, "child_v", v);
  collections.map_set(this as any, "seed", seedVal);
}

const c: number = new (Container as any)(7);
const seed: number = collections.map_get(c, "seed");
const childV: number = collections.map_get(c, "child_v");
print("c.seed=" + seed + " c.child_v=" + childV);

describe("new UserFn em loop e nested (#264 PR3)", () => {
  test("instances distintas no loop, this slot stack-based em nested", () =>
    expect(__rtsCapturedOutput).toBe(
      "total=100\n" +
      "c.seed=7 c.child_v=14\n"
    ));
});
