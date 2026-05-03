import { describe, test, expect } from "rts:test";
import { collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#261) `this` dentro de method shorthand referencia o object
// receiver via thread-local slot. Cada call empilha o receiver.

const counter: any = {
  v: 0,
  inc(): number {
    const t: number = (this as any);
    const cur: number = collections.map_get(t, "v");
    collections.map_set(t, "v", cur + 1);
    return cur + 1;
  },
};

const r1: number = counter.inc();
const r2: number = counter.inc();
const r3: number = counter.inc();
print("r1=" + r1);
print("r2=" + r2);
print("r3=" + r3);

// Diferentes objetos isolam estado
const counter2: any = {
  v: 100,
  inc(): number {
    const t: number = (this as any);
    const cur: number = collections.map_get(t, "v");
    collections.map_set(t, "v", cur + 1);
    return cur + 1;
  },
};

const a: number = counter2.inc();
const b: number = counter.inc();
print("a=" + a);
print("b=" + b);

describe("method shorthand this binding (#261)", () => {
  test("this referencia receiver, estado isolado entre objs", () =>
    expect(__rtsCapturedOutput).toBe(
      "r1=1\n" +
      "r2=2\n" +
      "r3=3\n" +
      "a=101\n" +
      "b=4\n"
    ));
});
