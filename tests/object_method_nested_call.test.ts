import { describe, test, expect } from "rts:test";
import { collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#261) Method de um obj chamando method de outro obj.
// THIS_PUSH eh stack-based — nesting nao quebra.

const inner: any = {
  marker: 99,
  read(): number {
    return collections.map_get((this as any), "marker");
  },
};

const outer: any = {
  marker: 7,
  combo(): number {
    const myMarker: number = collections.map_get((this as any), "marker");
    const innerMarker: number = inner.read();
    return myMarker * 100 + innerMarker;
  },
};

const r: number = outer.combo();
print("r=" + r);  // 7*100 + 99 = 799

// Confirma que outer ainda eh ele mesmo apos call retornar
const m: number = collections.map_get((outer as any), "marker");
print("m=" + m);

describe("method nested call (#261)", () => {
  test("stack this slot preserva receiver apos nested invoke", () =>
    expect(__rtsCapturedOutput).toBe(
      "r=799\n" +
      "m=7\n"
    ));
});
